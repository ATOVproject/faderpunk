use core::array;

use embassy_futures::{join::join, select::select};
use embassy_rp::clocks::RoscRng;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Sender,
    mutex::Mutex,
    pubsub::Subscriber,
    watch::Receiver,
};
use embassy_time::{with_timeout, Duration, Timer};
use max11300::config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midi2::{
    channel_voice1::{ChannelVoice1, ControlChange, NoteOff, NoteOn},
    ux::{u4, u7},
    Channeled,
};
use portable_atomic::Ordering;
use rand::Rng;

use config::Curve;
use libfp::{
    constants::{CHAN_LED_MAP, CURVE_EXP, CURVE_LOG},
    utils::u16_to_u7,
};

use crate::{
    tasks::{
        buttons::BUTTON_PRESSED,
        leds::{LedsAction, LED_VALUES},
        max::{MaxConfig, MaxMessage, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
    },
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

// FIXME: An app should be able to create at least as many waiters as it has channels multiplied
// by use cases

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

pub struct Buttons<const N: usize> {
    start_channel: usize,
}

impl<const N: usize> Buttons<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }

    /// Returns the number of the button that was pressed
    pub async fn wait_for_any_down(&self) -> usize {
        // Subscribers only listen on the start channel of an app
        let mut subscriber = CHANS_X[self.start_channel].subscriber().unwrap();

        loop {
            if let (channel, XTxMsg::ButtonDown) = subscriber.next_message_pure().await {
                return channel;
            }
        }
    }

    /// Returns if shift was pressed during button down
    pub async fn wait_for_down(&self, chan: usize) -> bool {
        loop {
            let channel = self.wait_for_any_down().await;
            if chan == channel {
                return self.is_shift_pressed();
            }
        }
    }

    pub async fn wait_for_long_press(&self, chan: usize, duration: Duration) -> bool {
        loop {
            let channel = self.wait_for_any_down().await;
            if chan == channel {
                Timer::after(duration).await;
                if BUTTON_PRESSED[self.start_channel + chan].load(Ordering::Relaxed) {
                    return self.is_shift_pressed();
                }
            }
        }
    }

    pub fn is_button_pressed(&self, chan: usize) -> bool {
        BUTTON_PRESSED[self.start_channel + chan].load(Ordering::Relaxed)
    }

    pub fn is_shift_pressed(&self) -> bool {
        BUTTON_PRESSED[17].load(Ordering::Relaxed)
    }
}

pub struct Faders<const N: usize> {
    start_channel: usize,
}

impl<const N: usize> Faders<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }

    pub async fn wait_for_change(&self, chan: usize) {
        loop {
            let channel = self.wait_for_any_change().await;
            if chan == channel {
                return;
            }
        }
    }

    /// Returns the number of the fader than was changed
    pub async fn wait_for_any_change(&self) -> usize {
        // Subscribers only listen on the start channel of an app
        let mut subscriber = CHANS_X[self.start_channel].subscriber().unwrap();
        loop {
            if let (channel, XTxMsg::FaderChange) = subscriber.next_message_pure().await {
                return channel;
            }
        }
    }

    pub fn get_values(&self) -> [u16; N] {
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = MAX_VALUES_FADER[self.start_channel + i].load(Ordering::Relaxed);
        }
        buf
    }
}

pub struct Clock {
    receiver: Receiver<'static, CriticalSectionRawMutex, bool, 16>,
}

impl Clock {
    pub fn default() -> Self {
        let receiver = CLOCK_WATCH.receiver().unwrap();
        Self { receiver }
    }

    pub async fn wait_for_tick(&mut self, division: usize) -> bool {
        let mut i: usize = 0;

        loop {
            // Reset always gets through
            if self.receiver.changed().await {
                return true;
            }
            i += 1;
            // TODO: Maybe we can make this more efficient by just having subscribers to
            // subdivisions of the clock
            if i == division {
                return false;
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

    // TODO: implement something like replace (using a closure)
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

pub struct Die {
    rng: RoscRng,
}

impl Die {
    /// Returns a random number between 0 and 4095
    pub fn roll(&mut self) -> u16 {
        self.rng.gen_range(0..=4095)
    }
}

pub struct App<const N: usize> {
    app_id: usize,
    start_channel: usize,
    channel_count: usize,
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
}

impl<const N: usize> App<N> {
    pub fn new(
        app_id: usize,
        start_channel: usize,
        sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
    ) -> Self {
        Self {
            app_id,
            start_channel,
            channel_count: N,
            sender,
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
            self.start_channel + chan,
            MaxConfig::Mode7(ConfigMode7(
                AVR::InternalRef,
                adc_range,
                NSAMPLES::Samples16,
            )),
        )
        .await;

        InJack::new(self.start_channel + chan, range)
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
            self.start_channel + chan,
            MaxConfig::Mode5(ConfigMode5(dac_range)),
        )
        .await;

        OutJack::new(self.start_channel + chan, range)
    }

    pub async fn make_gate_jack(&self, chan: usize, level: u16) -> GateJack {
        if chan > N - 1 {
            // TODO: Maybe move panics into usb logs and handle gracefully?
            panic!("Not a valid channel in this app");
        }

        self.reconfigure_jack(
            self.start_channel + chan,
            MaxConfig::Mode3(ConfigMode3, level),
        )
        .await;

        GateJack::new(self.start_channel + chan, self.sender)
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

    // TODO: Add effects
    // TODO: add methods to set brightness/color independently
    pub fn set_led(&self, chan: usize, position: Led, (r, g, b): (u8, u8, u8), brightness: u8) {
        let chan = self.start_channel + chan;
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
        // TODO: Make 32 a global config option
        cc.set_control(u7::new(32 + (self.start_channel + chan) as u8));
        cc.set_control_data(u16_to_u7(val));
        cc.set_channel(midi_channel);
        self.send_midi_msg(ChannelVoice1::ControlChange(cc)).await;
    }

    pub async fn midi_send_note_on(&self, note_number: u7, velocity: u16) {
        // TODO: Make configurable
        let midi_channel = u4::new(0);
        let mut note_on = NoteOn::<[u8; 3]>::new();
        note_on.set_channel(midi_channel);
        note_on.set_note_number(note_number);
        note_on.set_velocity(u16_to_u7(velocity));
        self.send_midi_msg(ChannelVoice1::NoteOn(note_on)).await;
    }

    pub async fn midi_send_note_off(&self, note_number: u7) {
        // TODO: Make configurable
        let midi_channel = u4::new(0);
        let mut note_off = NoteOff::<[u8; 3]>::new();
        note_off.set_channel(midi_channel);
        note_off.set_note_number(note_number);
        self.send_midi_msg(ChannelVoice1::NoteOff(note_off)).await;
    }

    // TODO: Check if making midi an own struct with a listener would make sense
    pub async fn send_midi_msg(&self, msg: ChannelVoice1<[u8; 3]>) {
        self.sender
            .send((self.start_channel, XRxMsg::MidiMessage(msg)))
            .await;
    }

    pub fn use_buttons(&self) -> Buttons<N> {
        Buttons::new(self.start_channel)
    }

    pub fn use_faders(&self) -> Faders<N> {
        Faders::new(self.start_channel)
    }

    pub fn use_die(&self) -> Die {
        Die { rng: RoscRng }
    }

    pub fn use_clock(&self) -> Clock {
        Clock::default()
    }
}
