use core::array;

use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Sender,
    mutex::Mutex,
    pubsub::Subscriber,
    watch::Receiver,
};
use embassy_time::Timer;
use max11300::config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midi2::{
    channel_voice1::{ChannelVoice1, ControlChange},
    ux::{u4, u7},
    Channeled,
};
use portable_atomic::Ordering;

use crate::{
    config::Curve,
    constants::{CHAN_LED_MAP, CURVE_EXP, CURVE_LOG},
    tasks::{
        buttons::BUTTON_PRESSED,
        leds::{LedsAction, LED_VALUES},
        max::{MaxConfig, MaxMessage, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
    },
    utils::u16_to_u7,
    XRxMsg, XTxMsg, CHANS_X, CLOCK_WATCH,
};

pub enum Range {
    // 0 - 10V
    _0_10V,
    // 0 - 5V
    _0_5V,
    // -5 - 5V
    _Neg5_5V,
}

pub enum Led {
    Top,
    Bottom,
    Button,
}

pub struct InJack {
    channel: usize,
    range: Range,
}

impl InJack {
    fn new(channel: usize, range: Range) -> Self {
        Self { channel, range }
    }

    pub fn get_value(&self) -> u16 {
        let val = MAX_VALUES_ADC[self.channel].load(Ordering::Relaxed);
        match self.range {
            Range::_0_5V => val.saturating_mul(2),
            _ => val,
        }
    }
}

pub struct GateJack {
    channel: usize,
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
}

impl GateJack {
    fn new(channel: usize, sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>) -> Self {
        Self { channel, sender }
    }

    pub async fn set_high(&self) {
        self.sender
            .send((self.channel, XRxMsg::MaxMessage(MaxMessage::GpoSetHigh)))
            .await;
    }

    pub async fn set_low(&self) {
        self.sender
            .send((self.channel, XRxMsg::MaxMessage(MaxMessage::GpoSetLow)))
            .await;
    }
}

pub struct OutJack {
    channel: usize,
    range: Range,
}

impl OutJack {
    fn new(channel: usize, range: Range) -> Self {
        Self { channel, range }
    }

    pub fn set_value(&self, value: u16) {
        let val = match self.range {
            Range::_0_5V => value / 2,
            _ => value,
        };
        MAX_VALUES_DAC[self.channel].store(val, Ordering::Relaxed);
    }

    pub fn set_value_with_curve(&self, curve: Curve, value: u16) {
        let transformed = match curve {
            Curve::Linear => value,
            Curve::Logarithmic => CURVE_LOG[value as usize],
            Curve::Exponential => CURVE_EXP[value as usize],
        };
        self.set_value(transformed);
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

    // Returns true if SHIFT is held at the same time
    pub async fn wait_for_button_down(&mut self, chan: usize) -> bool {
        loop {
            if let (channel, XTxMsg::ButtonDown) = self.subscriber.next_message_pure().await {
                if chan == channel {
                    return BUTTON_PRESSED[17].load(Ordering::Relaxed);
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
    clock_receiver: Receiver<'static, CriticalSectionRawMutex, bool, 16>,
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
            clock_receiver: CLOCK_WATCH.receiver().unwrap(),
        }
    }

    // TODO: We should also probably make sure that people do not reconfigure the jacks within the
    // app (throw error or something)
    async fn reconfigure_jack(&self, channel: usize, config: MaxConfig) {
        self.sender
            .send((
                channel,
                XRxMsg::MaxMessage(MaxMessage::ConfigurePort(config)),
            ))
            .await
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

    // TODO: How can we prevent people from doing this multiple times?
    pub async fn make_in_jack(&self, chan: usize, range: Range) -> InJack {
        if chan > N - 1 {
            // TODO: Maybe move panics into usb logs and handle gracefully?
            panic!("Not a valid channel in this app");
        }
        let adc_range = match range {
            Range::_Neg5_5V => ADCRANGE::RgNeg5_5v,
            _ => ADCRANGE::Rg0_10v,
        };
        self.reconfigure_jack(
            self.channels[chan],
            MaxConfig::Mode7(ConfigMode7(
                AVR::InternalRef,
                adc_range,
                NSAMPLES::Samples16,
            )),
        )
        .await;

        InJack::new(self.channels[chan], range)
    }

    // TODO: How can we prevent people from doing this multiple times?
    pub async fn make_out_jack(&self, chan: usize, range: Range) -> OutJack {
        if chan > N - 1 {
            // TODO: Maybe move panics into usb logs and handle gracefully?
            panic!("Not a valid channel in this app");
        }
        let dac_range = match range {
            Range::_Neg5_5V => DACRANGE::RgNeg5_5v,
            _ => DACRANGE::Rg0_10v,
        };
        self.reconfigure_jack(
            self.channels[chan],
            MaxConfig::Mode5(ConfigMode5(dac_range)),
        )
        .await;

        OutJack::new(self.channels[chan], range)
    }

    pub async fn make_gate_jack(&self, chan: usize, level: u16) -> GateJack {
        if chan > N - 1 {
            // TODO: Maybe move panics into usb logs and handle gracefully?
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(self.channels[chan], MaxConfig::Mode3(ConfigMode3, level))
            .await;

        GateJack::new(self.channels[chan], self.sender)
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

    //TODO: Check if app is CLOCK app and if not, do not implement this
    //HINT: Can we use struct markers? Or how to do it?
    pub async fn set_bpm(&self, bpm: f32) {
        self.sender.send((16, XRxMsg::SetBpm(bpm))).await;
    }

    pub fn is_button_pressed(&self, chan: usize) -> bool {
        BUTTON_PRESSED[self.channels[chan]].load(Ordering::Relaxed)
    }

    pub fn is_shift_pressed(&self) -> bool {
        BUTTON_PRESSED[17].load(Ordering::Relaxed)
    }

    // TODO: Add effects
    // TODO: add methods to set brightness/color independently
    pub fn set_led(&self, channel: usize, position: Led, (r, g, b): (u8, u8, u8), brightness: u8) {
        let chan = self.channels[channel];
        let led_no = match position {
            Led::Top => CHAN_LED_MAP[0][chan],
            Led::Bottom => CHAN_LED_MAP[1][chan],
            Led::Button => CHAN_LED_MAP[2][chan],
        };
        let value =
            ((brightness as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        LED_VALUES[led_no].store(value, Ordering::Relaxed);
    }

    // TODO: This is a short-hand function that should also send the msg via TRS
    // Create and use a function called midi_send_both and use it here
    pub async fn midi_send_cc(&self, chan: usize, val: u16) {
        // TODO: Make configurable
        let midi_channel = u4::new(0);
        let mut cc = ControlChange::<[u8; 3]>::new();
        cc.set_control(u7::new(32 + self.channels[chan] as u8));
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

    pub async fn wait_for_clock(&mut self, division: usize) {
        let mut i: usize = 0;
        loop {
            self.clock_receiver.changed().await;
            i += 1;
            if i == division {
                return;
            }
        }
    }
}
