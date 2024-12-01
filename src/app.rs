use core::{array, future::Future};

use defmt::info;
use embassy_futures::join::join;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::Subscriber};
use embassy_time::Timer;
use max11300::config::{ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use portable_atomic::Ordering;
use wmidi::{Channel, ControlFunction, MidiMessage, U7};

use crate::tasks::{
    buttons::BUTTON_PUBSUB,
    leds::{LedsAction, CHANNEL_LEDS},
    max::{
        MaxConfig, MaxReconfigureAction, MAX_CHANNEL_RECONFIGURE, MAX_MASK_RECONFIGURE,
        MAX_PUBSUB_FADER_CHANGED, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADERS,
    },
    serial::{UartAction, CHANNEL_UART_TX},
    usb::{UsbAction, CHANNEL_USB_TX, USB_CONNECTED},
};

// FIXME: put this into some util create
fn u16_to_u7(value: u16) -> U7 {
    U7::from_u8_lossy(((value as u32 * 127) / 65535) as u8)
}

pub struct InJack {
    channel: usize,
}

impl InJack {
    pub async fn get_value(&self) -> u16 {
        let adc_values = MAX_VALUES_ADC.lock().await;
        adc_values[self.channel]
    }
}

pub struct OutJack {
    channel: usize,
}

impl OutJack {
    pub async fn set_value(&self, value: u16) {
        let mut dac_values = MAX_VALUES_DAC.lock().await;
        dac_values[self.channel] = Some(value);
    }
}

pub struct InJacks<const N: usize> {
    channels: [usize; N],
}

impl<const N: usize> InJacks<N> {
    pub async fn get_values(&self) -> [u16; N] {
        let adc_values = MAX_VALUES_ADC.lock().await;
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = adc_values[self.channels[i]];
        }
        buf
    }
}

pub struct OutJacks<const N: usize> {
    channels: [usize; N],
}

impl<const N: usize> OutJacks<N> {
    pub async fn set_values(&self, values: [u16; N]) {
        let mut dac_values = MAX_VALUES_DAC.lock().await;
        for i in 0..N {
            dac_values[self.channels[i]] = Some(values[i]);
        }
    }
}

pub struct FaderWaiter<'a> {
    channel: usize,
    subscriber: Subscriber<'a, CriticalSectionRawMutex, usize, 4, 16, 1>,
}

impl<'a> FaderWaiter<'a> {
    pub fn new(channel: usize) -> Self {
        let subscriber = MAX_PUBSUB_FADER_CHANGED.subscriber().unwrap();
        Self {
            channel,
            subscriber,
        }
    }
    pub async fn wait_for_fader_change(&mut self) {
        loop {
            let notified_channel = self.subscriber.next_message_pure().await;
            if self.channel == notified_channel {
                return;
            }
        }
    }
}

pub struct ButtonWaiter<'a> {
    channel: usize,
    subscriber: Subscriber<'a, CriticalSectionRawMutex, usize, 4, 16, 1>,
}

impl<'a> ButtonWaiter<'a> {
    pub fn new(channel: usize) -> Self {
        let subscriber = BUTTON_PUBSUB.subscriber().unwrap();
        Self {
            channel,
            subscriber,
        }
    }
    pub async fn wait_for_button_press(&mut self) {
        loop {
            let notified_channel = self.subscriber.next_message_pure().await;
            if self.channel == notified_channel {
                return;
            }
        }
    }
}

pub struct App<const N: usize> {
    app_id: usize,
    channels: [usize; N],
}

impl<const N: usize> App<N> {
    pub fn new(app_id: usize, start_channel: usize) -> Self {
        // Create an array of all channels numbers that this app is using
        let channels: [usize; N] = array::from_fn(|i| start_channel + i);
        Self { app_id, channels }
    }

    pub fn size() -> usize {
        N
    }

    pub async fn get_fader_values(&self) -> [u16; N] {
        let fader_values = MAX_VALUES_FADERS.lock().await;
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = fader_values[self.channels[i]];
        }
        buf
    }

    // FIXME: We should also probably make sure that people do not reconfigure the jacks within the
    // app (throw error or something)
    pub async fn make_in_jack(&self, chan: usize) -> InJack {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(
            // FIXME: it's also terrible that we have to pass the channel twice here
            self.channels[chan],
            (
                self.channels[chan],
                MaxConfig::Mode7(ConfigMode7(
                    AVR::InternalRef,
                    ADCRANGE::Rg0_10v,
                    NSAMPLES::Samples16,
                )),
            ),
        )
        .await;

        InJack { channel: chan }
    }

    pub async fn make_out_jack(&self, chan: usize) -> OutJack {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(
            // FIXME: it's also terrible that we have to pass the channel twice here
            self.channels[chan],
            (
                self.channels[chan],
                MaxConfig::Mode5(ConfigMode5(DACRANGE::Rg0_10v)),
            ),
        )
        .await;

        OutJack { channel: chan }
    }

    pub async fn make_all_in_jacks(&self) -> InJacks<N> {
        // FIXME: add a configure_jacks function that can configure multiple jacks at once (using
        // the multiport feature of the MAX)
        for channel in self.channels {
            self.reconfigure_jack(
                channel,
                (
                    channel,
                    MaxConfig::Mode7(ConfigMode7(
                        AVR::InternalRef,
                        ADCRANGE::Rg0_10v,
                        NSAMPLES::Samples16,
                    )),
                ),
            )
            .await;
        }

        InJacks {
            channels: self.channels,
        }
    }

    // FIXME: Here we actually need to reconfigure the jacks
    pub async fn make_all_out_jacks(&self) -> OutJacks<N> {
        // FIXME: add a configure_jacks function that can configure multiple jacks at once (using
        // the multiport feature of the MAX)
        for channel in self.channels {
            self.reconfigure_jack(
                channel,
                (channel, MaxConfig::Mode5(ConfigMode5(DACRANGE::Rg0_10v))),
            )
            .await;
        }

        OutJacks {
            channels: self.channels,
        }
    }

    pub async fn delay_micros(&self, micros: u64) {
        Timer::after_micros(micros).await
    }

    pub async fn delay_millis(&self, millis: u64) {
        Timer::after_millis(millis).await
    }

    pub async fn delay_secs(&self, secs: u64) {
        Timer::after_secs(secs).await
    }

    pub async fn led_blink(&self, chan: usize, duration: u64) {
        // FIXME: We're doing this a lot, let's abstract this
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }
        let channel = self.channels[chan];
        CHANNEL_LEDS
            .send((channel, LedsAction::Blink(duration)))
            .await;
    }

    // FIXME: This is a short-hand function that should also send the msg via TRS
    // Create and use a function called midi_send_both and use it here
    pub async fn midi_send_cc(&self, chan: Channel, cc: ControlFunction, val: u16) {
        let msg = MidiMessage::ControlChange(chan, cc, u16_to_u7(val));
        self.send_midi_msg(msg).await;
    }

    pub async fn send_midi_msg(&self, msg: MidiMessage<'_>) {
        let uart_fut = CHANNEL_UART_TX.send(UartAction::SendMidiMsg(msg.to_owned()));
        if USB_CONNECTED.load(Ordering::Relaxed) {
            join(
                CHANNEL_USB_TX.send(UsbAction::SendMidiMsg(msg.to_owned())),
                uart_fut,
            )
            .await;
        } else {
            uart_fut.await;
        }
    }

    pub fn make_fader_waiter(&self, chan: usize) -> FaderWaiter {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }
        FaderWaiter::new(self.channels[chan])
    }

    pub fn make_button_waiter(&self, chan: usize) -> ButtonWaiter {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }
        ButtonWaiter::new(self.channels[chan])
    }

    async fn reconfigure_jack(&self, channel: usize, action: MaxReconfigureAction) {
        MAX_CHANNEL_RECONFIGURE.send(action).await;
        loop {
            // See if the reconfiguration is done
            self.delay_millis(10).await;
            let mask = 1 << (channel) as u16;
            if (MAX_MASK_RECONFIGURE.load(Ordering::Relaxed) & mask) != 0 {
                MAX_MASK_RECONFIGURE.fetch_and(!mask, Ordering::SeqCst);
                break;
            }
        }
    }
}
