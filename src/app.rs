use core::array;

use defmt::info;
use embassy_futures::join::join;
use embassy_sync::{
    blocking_mutex::raw::{NoopRawMutex, ThreadModeRawMutex},
    channel::Sender,
    mutex::Mutex,
    pubsub::Subscriber,
};
use embassy_time::Timer;
use max11300::config::{ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midi2::{
    channel_voice1::{ChannelVoice1, ControlChange},
    ux::{u4, u7},
    Channeled,
};
use portable_atomic::Ordering;

use crate::{
    config::Curve,
    constants::{CURVE_EXP, CURVE_LOG},
    tasks::{
        leds::{LedsAction, LED_VALUES},
        max::{MaxConfig, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
    },
    XRxMsg, XTxMsg, CHANS_X,
};

// TODO: put this into utils
/// Scale from 4096 to 127
fn u16_to_u7(value: u16) -> u7 {
    u7::new(((value as u32 * 127) / 4095) as u8)
}

pub struct InJack {
    channel: usize,
}

impl InJack {
    pub fn get_value(&self) -> u16 {
        MAX_VALUES_ADC[self.channel].load(Ordering::Relaxed)
    }
}

pub struct OutJack {
    channel: usize,
}

impl OutJack {
    pub fn set_value(&self, value: u16) {
        MAX_VALUES_DAC[self.channel].store(value, Ordering::Relaxed);
    }
}

pub struct InJacks<const N: usize> {
    channels: [usize; N],
}

impl<const N: usize> InJacks<N> {
    pub fn get_values(&self) -> [u16; N] {
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = MAX_VALUES_ADC[self.channels[i]].load(Ordering::Relaxed);
        }
        buf
    }
}

pub struct OutJacks<const N: usize> {
    channels: [usize; N],
}

impl<const N: usize> OutJacks<N> {
    pub fn set_values(&self, values: [u16; N]) {
        for (i, &chan) in self.channels.iter().enumerate() {
            MAX_VALUES_DAC[chan].store(values[i], Ordering::Relaxed);
        }
    }

    pub fn set_values_with_curve(&self, curve: Curve, values: [u16; N]) {
        let transfomed = values.map(|val| match curve {
            Curve::Linear => val,
            Curve::Logarithmic => CURVE_LOG[val as usize],
            Curve::Exponential => CURVE_EXP[val as usize],
        });
        self.set_values(transfomed);
    }
}

pub struct Waiter {
    subscriber: Subscriber<'static, ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>,
}

impl Waiter {
    pub fn new(
        subscriber: Subscriber<'static, ThreadModeRawMutex, (usize, XTxMsg), 64, 5, 1>,
    ) -> Self {
        Self { subscriber }
    }

    pub async fn wait_for_fader_change(&mut self, chan: usize) {
        loop {
            if let (channel, XTxMsg::FaderChange) = self.subscriber.next_message_pure().await {
                if chan == channel {
                    return;
                }
            }
        }
    }
    pub async fn wait_for_button_down(&mut self, chan: usize) {
        loop {
            if let (channel, XTxMsg::ButtonDown) = self.subscriber.next_message_pure().await {
                if chan == channel {
                    return;
                }
            }
        }
    }
    pub async fn wait_for_int_clock(&mut self, chan: usize) {
        loop {
            if let (channel, XTxMsg::ClockInt) = self.subscriber.next_message_pure().await {
                if chan == channel {
                    return;
                }
            }
        }
    }
}

pub struct Global<T: Sized + Copy> {
    mutex: Mutex<NoopRawMutex, T>,
}

impl<T: Sized + Copy> Global<T> {
    pub fn new(initial: T) -> Self {
        Self {
            mutex: Mutex::new(initial),
        }
    }

    pub async fn get(&self) -> T {
        let value = self.mutex.lock().await;
        *value
    }

    pub async fn set(&self, val: T) {
        let mut value = self.mutex.lock().await;
        *value = val
    }
}

impl Global<bool> {
    pub async fn toggle(&self) -> bool {
        let mut value = self.mutex.lock().await;
        *value = !*value;
        *value
    }
}

pub struct App<const N: usize> {
    app_id: usize,
    pub channels: [usize; N],
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
}

impl<const N: usize> App<N> {
    pub fn new(
        app_id: usize,
        start_channel: usize,
        sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
    ) -> Self {
        // Create an array of all channels numbers that this app is using
        let channels: [usize; N] = array::from_fn(|i| start_channel + i);
        Self {
            app_id,
            channels,
            sender,
        }
    }

    pub fn make_global<T: Sized + Copy>(&self, initial: T) -> Global<T> {
        Global::new(initial)
    }

    pub fn get_fader_values(&self) -> [u16; N] {
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = MAX_VALUES_FADER[self.channels[i]].load(Ordering::Relaxed);
        }
        buf
    }

    // TODO: We should also probably make sure that people do not reconfigure the jacks within the
    // app (throw error or something)
    pub async fn make_in_jack(&self, chan: usize) -> InJack {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(
            self.channels[chan],
            MaxConfig::Mode7(ConfigMode7(
                AVR::InternalRef,
                ADCRANGE::Rg0_10v,
                NSAMPLES::Samples16,
            )),
        )
        .await;

        InJack { channel: chan }
    }

    pub async fn make_out_jack(&self, chan: usize) -> OutJack {
        if chan > N - 1 {
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(
            self.channels[chan],
            MaxConfig::Mode5(ConfigMode5(DACRANGE::Rg0_10v)),
        )
        .await;

        OutJack { channel: chan }
    }

    pub async fn make_all_in_jacks(&self) -> InJacks<N> {
        // TODO: add a configure_jacks function that can configure multiple jacks at once (using
        // the multiport feature of the MAX)
        for channel in self.channels {
            self.reconfigure_jack(
                channel,
                MaxConfig::Mode7(ConfigMode7(
                    AVR::InternalRef,
                    ADCRANGE::Rg0_10v,
                    NSAMPLES::Samples16,
                )),
            )
            .await;
        }

        InJacks {
            channels: self.channels,
        }
    }

    // TODO: Store internally which jacks have been configured
    pub async fn make_all_out_jacks(&self) -> OutJacks<N> {
        // TODO: add a configure_jacks function that can configure multiple jacks at once (using
        // the multiport feature of the MAX)
        for channel in self.channels {
            self.reconfigure_jack(channel, MaxConfig::Mode5(ConfigMode5(DACRANGE::Rg0_10v)))
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

    // TODO: Currently only the setting of raw values is possible
    // TODO: Also add a custom flush() method and so on
    pub async fn set_led(&self, channel: usize, val: u32) {
        LED_VALUES[self.channels[channel]].store(val, Ordering::Relaxed);
        self.sender
            .send((self.channels[channel], XRxMsg::SetLed(LedsAction::Flush)))
            .await;
    }

    pub async fn set_bpm(&self, bpm: u16) {
        self.sender
            .send((self.channels[0], XRxMsg::SetBpm(bpm)))
            .await;
    }

    // pub async fn led_blink(&self, chan: usize, duration: u64) {
    //     // TODO: We're doing this a lot, let's abstract this
    //     if chan > N - 1 {
    //         panic!("Not a valid channel in this app");
    //     }
    //     let channel = self.channels[chan];
    //     CHANNEL_LEDS
    //         .send((channel, LedsAction::Blink(duration)))
    //         .await;
    // }

    // TODO: This is a short-hand function that should also send the msg via TRS
    // Create and use a function called midi_send_both and use it here
    pub async fn midi_send_cc(&self, chan: usize, val: u16) {
        // TODO: Make configurable
        let midi_channel = u4::new(1);
        let mut cc = ControlChange::<[u8; 3]>::new();
        cc.set_control(u16_to_u7(102 + chan as u16));
        cc.set_control_data(u16_to_u7(val));
        cc.set_channel(midi_channel);
        let msg = ChannelVoice1::ControlChange(cc);
        self.send_midi_msg(msg).await;
    }

    pub async fn send_midi_msg(&self, msg: ChannelVoice1<[u8; 3]>) {
        self.sender
            .send((self.channels[0], XRxMsg::MidiMessage(msg)))
            .await;
    }

    pub fn make_waiter(&self) -> Waiter {
        // Subscribers only listen on the start channel of an app
        let subscriber = CHANS_X[self.channels[0]].subscriber().unwrap();
        Waiter::new(subscriber)
    }

    async fn reconfigure_jack(&self, channel: usize, config: MaxConfig) {
        self.sender
            .send((channel, XRxMsg::MaxPortReconfigure(config)))
            .await
    }
}
