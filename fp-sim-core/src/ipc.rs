use std::io::BufWriter;
use std::sync::{mpsc, OnceLock};

use embassy_time::Timer;
use portable_atomic::Ordering;

use fp_core::tasks::clock::{TransportCmd, CLOCK_RUNNING, TRANSPORT_CMD_CHANNEL};
use fp_core::tasks::global_config::get_global_config;
use fp_core::tasks::input_handlers::current_scene;
use fp_core::tasks::max::{MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER};
use fp_sim_protocol::{CoreSnapshot, CoreToHost, HostToCore, CHANNELS, LEDS, PORTS};

use crate::hw::{GATE_STATES, LED_FRAME, PORT_MODES, PORT_RANGES};
use crate::panel::{set_button, SIM_FADER_POS};

static OUTPUT: OnceLock<mpsc::Sender<CoreToHost>> = OnceLock::new();

pub fn init() {
    let (sender, receiver) = mpsc::channel();
    OUTPUT.set(sender).expect("IPC output already initialized");
    std::thread::Builder::new()
        .name("fp-sim-ipc-out".into())
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut writer = BufWriter::new(stdout.lock());
            for message in receiver {
                if fp_sim_protocol::write_frame(&mut writer, &message).is_err() {
                    break;
                }
            }
        })
        .expect("failed to start IPC output thread");

    std::thread::Builder::new()
        .name("fp-sim-ipc-in".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            loop {
                match fp_sim_protocol::read_frame::<HostToCore>(&mut reader) {
                    Ok(Some(message)) => handle_input(message),
                    Ok(None) => std::process::exit(0),
                    Err(err) => {
                        log::error!("Invalid host IPC frame: {err}");
                        std::process::exit(2);
                    }
                }
            }
        })
        .expect("failed to start IPC input thread");
}

pub fn send(message: CoreToHost) {
    if let Some(output) = OUTPUT.get() {
        let _ = output.send(message);
    }
}

fn handle_input(message: HostToCore) {
    match message {
        HostToCore::Fader { channel, value } if (channel as usize) < CHANNELS => {
            SIM_FADER_POS[channel as usize].store(value.min(4095), Ordering::Relaxed);
        }
        HostToCore::Button { index, pressed } if (index as usize) < 18 => {
            set_button(index as usize, pressed);
        }
        HostToCore::Adc { port, value } if (port as usize) < PORTS => {
            MAX_VALUES_ADC[port as usize].store(value.min(4095), Ordering::Relaxed);
        }
        HostToCore::TransportToggle => {
            let _ = TRANSPORT_CMD_CHANNEL.try_send(TransportCmd::Toggle);
        }
        HostToCore::PerformanceMidi(bytes) => {
            if !crate::midi::push_performance(&bytes) {
                log::warn!("Performance MIDI IPC queue full, dropping input");
            }
        }
        HostToCore::ConfigMidi(bytes) => {
            if !crate::midi::push_config(&bytes) {
                log::warn!("Config MIDI IPC queue full, dropping input");
            }
        }
        HostToCore::Shutdown => std::process::exit(0),
        HostToCore::Fader { .. } | HostToCore::Button { .. } | HostToCore::Adc { .. } => {
            log::warn!("Ignoring out-of-range panel IPC input");
        }
    }
}

#[embassy_executor::task]
pub async fn publish_state() {
    loop {
        Timer::after_millis(16).await;
        let config = get_global_config();
        send(CoreToHost::Snapshot(CoreSnapshot {
            leds: (0..LEDS)
                .map(|index| LED_FRAME[index].load(Ordering::Relaxed))
                .collect(),
            latched_faders: core::array::from_fn(|index| {
                MAX_VALUES_FADER[index].load(Ordering::Relaxed)
            }),
            adc: core::array::from_fn(|index| MAX_VALUES_ADC[index].load(Ordering::Relaxed)),
            dac: core::array::from_fn(|index| MAX_VALUES_DAC[index].load(Ordering::Relaxed)),
            port_modes: core::array::from_fn(|index| PORT_MODES[index].load(Ordering::Relaxed)),
            port_ranges: core::array::from_fn(|index| PORT_RANGES[index].load(Ordering::Relaxed)),
            gates: core::array::from_fn(|index| GATE_STATES[index].load(Ordering::Relaxed)),
            clock_running: CLOCK_RUNNING.load(Ordering::Relaxed),
            current_scene: current_scene(),
            bpm: config.clock.internal_bpm,
            swing: config.clock.swing_amount,
        }));
    }
}
