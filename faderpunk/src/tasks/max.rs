use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Level, Output},
    peripherals::{PIN_12, PIN_13, PIN_14, PIN_15, PIN_17, PIO0, SPI0},
    pio::{Config as PioConfig, Direction as PioDirection, Pio},
    spi::{self, Async, Spi},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::Timer;
use max11300::{
    config::{
        ConfigMode0, ConfigMode3, ConfigMode5, ConfigMode7, DeviceConfig, Port, ADCCTL, ADCRANGE,
        AVR, DACREF, NSAMPLES, THSHDN,
    },
    ConfigurePort, IntoConfiguredPort, Max11300, Mode0Port, Ports,
};
use portable_atomic::{AtomicU16, Ordering};
use static_cell::StaticCell;

use crate::{
    events::{InputEvent, EVENT_PUBSUB},
    Irqs,
};

const MIDI_CHANNEL_SIZE: usize = 16;

type SharedMax = Mutex<NoopRawMutex, Max11300<Spi<'static, SPI0, Async>, Output<'static>>>;
type MuxPins = (PIN_12, PIN_13, PIN_14, PIN_15);

pub type MaxSender = Sender<'static, CriticalSectionRawMutex, (usize, MaxCmd), MIDI_CHANNEL_SIZE>;

static MAX: StaticCell<SharedMax> = StaticCell::new();
pub static MAX_VALUES_DAC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_FADER: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_VALUES_ADC: [AtomicU16; 16] = [const { AtomicU16::new(0) }; 16];
pub static MAX_CHANNEL: Channel<CriticalSectionRawMutex, (usize, MaxCmd), 16> = Channel::new();

#[derive(Clone, Copy)]
pub enum MaxCmd {
    ConfigurePort(MaxConfig),
    GpoSetHigh,
    GpoSetLow,
}

#[derive(Clone, Copy)]
pub enum MaxConfig {
    Mode0,
    Mode3(ConfigMode3, u16),
    Mode5(ConfigMode5),
    Mode7(ConfigMode7),
}

pub async fn start_max(
    spawner: &Spawner,
    spi0: Spi<'static, SPI0, spi::Async>,
    pio0: PIO0,
    mux_pins: MuxPins,
    cs: PIN_17,
) {
    let device_config = DeviceConfig {
        thshdn: THSHDN::Enabled,
        dacref: DACREF::InternalRef,
        adcctl: ADCCTL::ContinuousSweep,
        ..Default::default()
    };

    let max_driver = Max11300::try_new(spi0, Output::new(cs, Level::High), device_config)
        .await
        .unwrap();

    let max = MAX.init(Mutex::new(max_driver));

    // TODO: Create an abstraction to be able to create just one port
    let ports = Ports::new(max);

    // Put ports 17-19 into hi-impedance mode for interrupt testing
    ports
        .port17
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();
    ports
        .port18
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();
    ports
        .port19
        .into_configured_port(ConfigMode0)
        .await
        .unwrap();

    // TODO: Make individual port
    spawner
        .spawn(read_fader(pio0, mux_pins, ports.port16))
        .unwrap();

    spawner.spawn(process_channel_values(max)).unwrap();

    spawner.spawn(message_loop(max)).unwrap();
}

#[embassy_executor::task]
async fn read_fader(
    pio0: PIO0,
    mux_pins: MuxPins,
    max_port: Mode0Port<Spi<'static, SPI0, spi::Async>, Output<'static>, NoopRawMutex>,
) {
    let event_publisher = EVENT_PUBSUB.publisher().unwrap();

    let fader_port = max_port
        .into_configured_port(ConfigMode7(
            AVR::InternalRef,
            ADCRANGE::Rg0_2v5,
            NSAMPLES::Samples16,
        ))
        .await
        .unwrap();

    let Pio {
        mut common,
        mut sm0,
        ..
    } = Pio::new(pio0, Irqs);

    let prg = pio::pio_asm!(
        "
        pull block
        out pins, 4
        "
    );
    let pin0 = common.make_pio_pin(mux_pins.0);
    let pin1 = common.make_pio_pin(mux_pins.1);
    let pin2 = common.make_pio_pin(mux_pins.2);
    let pin3 = common.make_pio_pin(mux_pins.3);
    sm0.set_pin_dirs(PioDirection::Out, &[&pin0, &pin1, &pin2, &pin3]);
    let mut cfg = PioConfig::default();
    cfg.set_out_pins(&[&pin0, &pin1, &pin2, &pin3]);
    cfg.use_program(&common.load_program(&prg.program), &[]);
    sm0.set_config(&cfg);
    sm0.set_enable(true);

    let mut chan: usize = 0;
    let mut prev_values: [u16; 16] = [0; 16];

    loop {
        // Channels are in reverse
        let channel = 15 - chan;
        // send the channel value to the PIO state machine to trigger the program
        sm0.tx().wait_push(chan as u32).await;

        // this translates to ~30Hz refresh rate for the faders (1000 / (2 * 16) = 31.25)
        // TODO: Why is this sometimes too short?
        // Like with scene_sender.send(&[1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2]);
        Timer::after_micros(3000).await;

        let val = fader_port.get_value().await.unwrap();
        let diff = (val as i16 - prev_values[channel] as i16).unsigned_abs();
        // resolution of 256 should cover the full MIDI 1.0 range
        prev_values[channel] = val;

        if diff >= 4 {
            event_publisher
                .publish(InputEvent::FaderChange(channel))
                .await;
        }

        MAX_VALUES_FADER[channel].store(val, Ordering::Relaxed);

        chan = (chan + 1) % 16;
    }
}

// TODO: Should we make this message based?
#[embassy_executor::task]
async fn process_channel_values(max_driver: &'static SharedMax) {
    loop {
        // hopefully we can write it at about 2kHz
        Timer::after_micros(500).await;

        for i in 0..16 {
            let port = Port::try_from(i).unwrap();
            let mut max = max_driver.lock().await;
            match max.get_mode(port) {
                5 => {
                    let value = MAX_VALUES_DAC[i].load(Ordering::Relaxed);
                    max.dac_set_value(port, value).await.unwrap();
                }
                7 => {
                    let value = max.adc_get_value(port).await.unwrap();
                    MAX_VALUES_ADC[i].store(value, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }
}

#[embassy_executor::task]
async fn message_loop(max_driver: &'static SharedMax) {
    // TODO: Put ports 17-19 in hi-impedance mode when using the internal GPIO interrupts
    loop {
        let (chan, msg) = MAX_CHANNEL.receive().await;
        let port = Port::try_from(chan).unwrap();
        let mut max = max_driver.lock().await;

        match msg {
            MaxCmd::ConfigurePort(config) => match config {
                MaxConfig::Mode0 => {
                    max.configure_port(port, ConfigMode0).await.unwrap();
                }
                MaxConfig::Mode3(config, gpo_level) => {
                    max.dac_set_value(port, gpo_level).await.unwrap();
                    max.configure_port(port, config).await.unwrap();
                }
                MaxConfig::Mode5(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
                MaxConfig::Mode7(config) => {
                    max.configure_port(port, config).await.unwrap();
                }
            },
            MaxCmd::GpoSetHigh => {
                max.gpo_set_high(port).await.unwrap();
            }
            MaxCmd::GpoSetLow => {
                max.gpo_set_low(port).await.unwrap();
            }
        }
    }
}
