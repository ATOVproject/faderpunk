use embassy_rp::clocks::RoscRng;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer};
use max11300::config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midly::{live::LiveEvent, num::u4, MidiMessage};
use portable_atomic::Ordering;
use rand::Rng;

use config::Curve;
use libfp::{
    constants::{CHAN_LED_MAP, CURVE_EXP, CURVE_LOG},
    utils::scale_bits_12_7,
};

use crate::{
    tasks::{
        buttons::BUTTON_PRESSED,
        clock::{ClockSubscriber, CLOCK_PUBSUB},
        leds::LED_VALUES,
        max::{MaxCmd, MaxConfig, MaxSender, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
        midi::MidiSender,
    },
    EventPubSubChannel, InputEvent,
};

pub use crate::{
    storage::{AppStorage, Arr, ManagedStorage, ParamSlot, ParamStore},
    tasks::clock::ClockEvent,
};

// TODO: This will be refactored using an allocator
const BYTES_PER_VALUE_SET: u32 = 1000;
const SCENES_PER_APP: u32 = 3;

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

#[derive(Clone, Copy)]
pub struct Leds<const N: usize> {
    start_channel: usize,
}

impl<const N: usize> Leds<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }

    // TODO: Add effects
    // TODO: add methods to set brightness/color independently
    pub fn set(&self, chan: usize, position: Led, (r, g, b): (u8, u8, u8), brightness: u8) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        let led_no = match position {
            Led::Top => CHAN_LED_MAP[0][channel],
            Led::Bottom => CHAN_LED_MAP[1][channel],
            Led::Button => CHAN_LED_MAP[2][channel],
        };
        let value =
            ((brightness as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        LED_VALUES[led_no].store(value, Ordering::Relaxed);
    }

    pub fn reset_all(&self) {
        for chan in 0..N {
            self.set(chan, Led::Button, (0, 0, 0), 0);
            self.set(chan, Led::Bottom, (0, 0, 0), 0);
            self.set(chan, Led::Top, (0, 0, 0), 0);
        }
    }
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
    max_sender: MaxSender,
}

impl GateJack {
    fn new(channel: usize, max_sender: MaxSender) -> Self {
        Self {
            channel,
            max_sender,
        }
    }

    pub async fn set_high(&self) {
        self.max_sender
            .send((self.channel, MaxCmd::GpoSetHigh))
            .await;
    }

    pub async fn set_low(&self) {
        self.max_sender
            .send((self.channel, MaxCmd::GpoSetLow))
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

#[derive(Clone, Copy)]
pub struct Buttons<const N: usize> {
    event_pubsub: &'static EventPubSubChannel,
    start_channel: usize,
}

impl<const N: usize> Buttons<N> {
    pub fn new(start_channel: usize, event_pubsub: &'static EventPubSubChannel) -> Self {
        Self {
            event_pubsub,
            start_channel,
        }
    }

    /// Returns the number of the button that was pressed
    pub async fn wait_for_any_down(&self) -> usize {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::ButtonDown(channel) = subscriber.next_message_pure().await {
                if (self.start_channel..self.start_channel + N).contains(&channel) {
                    return channel - self.start_channel;
                }
            }
        }
    }

    /// Returns if shift was pressed during button down
    pub async fn wait_for_down(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let channel = self.wait_for_any_down().await;
            if chan == channel {
                return self.is_shift_pressed();
            }
        }
    }

    pub async fn wait_for_any_long_press(&self, duration: Duration) -> usize {
        loop {
            let channel = self.wait_for_any_down().await;
            Timer::after(duration).await;
            if BUTTON_PRESSED[self.start_channel + channel].load(Ordering::Relaxed) {
                return channel;
            }
        }
    }

    pub async fn wait_for_long_press(&self, chan: usize, duration: Duration) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let channel = self.wait_for_any_long_press(duration).await;
            if chan == channel {
                return self.is_shift_pressed();
            }
        }
    }

    pub fn is_button_pressed(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        BUTTON_PRESSED[self.start_channel + chan].load(Ordering::Relaxed)
    }

    pub fn is_shift_pressed(&self) -> bool {
        BUTTON_PRESSED[17].load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy)]
pub struct Faders<const N: usize> {
    event_pubsub: &'static EventPubSubChannel,
    start_channel: usize,
}

impl<const N: usize> Faders<N> {
    pub fn new(start_channel: usize, event_pubsub: &'static EventPubSubChannel) -> Self {
        Self {
            event_pubsub,
            start_channel,
        }
    }

    /// Returns the number of the fader than was changed
    pub async fn wait_for_any_change(&self) -> usize {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::FaderChange(channel) = subscriber.next_message_pure().await {
                if (self.start_channel..self.start_channel + N).contains(&channel) {
                    return channel - self.start_channel;
                }
            }
        }
    }

    pub async fn wait_for_change(&self, chan: usize) {
        let chan = chan.clamp(0, N - 1);
        loop {
            let channel = self.wait_for_any_change().await;
            if chan == channel {
                return;
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
    subscriber: ClockSubscriber,
}

impl Clock {
    pub fn new() -> Self {
        let subscriber = CLOCK_PUBSUB.subscriber().unwrap();
        Self { subscriber }
    }

    // TODO: division needs to be an enum
    pub async fn wait_for_event(&mut self, division: usize) -> ClockEvent {
        let mut i: usize = 0;

        loop {
            match self.subscriber.next_message_pure().await {
                ClockEvent::Tick => {
                    i += 1;
                    // TODO: Maybe we can make this more efficient by just having subscribers to
                    // subdivisions of the clock
                    if i == division {
                        return ClockEvent::Tick;
                    }
                    continue;
                }
                clock_event => {
                    return clock_event;
                }
            }
        }
    }
}

pub enum SceneEvent {
    LoadSscene(u8),
    SaveScene(u8),
}

#[derive(Clone, Copy)]
pub struct Midi<const N: usize> {
    midi_sender: MidiSender,
    midi_channel: u4,
}

impl<const N: usize> Midi<N> {
    pub fn new(midi_channel: u4, midi_sender: MidiSender) -> Self {
        Self {
            midi_sender,
            midi_channel,
        }
    }

    pub async fn send_cc(&self, cc: u8, val: u16) {
        let msg = LiveEvent::Midi {
            channel: self.midi_channel,
            message: MidiMessage::Controller {
                controller: cc.into(),
                value: scale_bits_12_7(val),
            },
        };
        self.send_msg(msg).await;
    }

    pub async fn send_note_on(&self, note_number: u8, velocity: u16) {
        let msg = LiveEvent::Midi {
            channel: self.midi_channel,
            message: MidiMessage::NoteOn {
                key: note_number.into(),

                vel: scale_bits_12_7(velocity),
            },
        };
        self.send_msg(msg).await;
    }

    pub async fn send_note_off(&self, note_number: u8) {
        let msg = LiveEvent::Midi {
            channel: self.midi_channel,
            message: MidiMessage::NoteOff {
                key: note_number.into(),
                vel: 0.into(),
            },
        };
        self.send_msg(msg).await;
    }

    pub async fn send_msg(&self, msg: LiveEvent<'static>) {
        self.midi_sender.send(msg).await;
    }
}

pub struct Global<T: Sized> {
    inner: Mutex<NoopRawMutex, T>,
}

impl<T: Sized + Copy> Global<T> {
    pub fn new(initial: T) -> Self {
        Self {
            inner: Mutex::new(initial),
        }
    }

    // TODO: implement something like replace (using a closure)
    pub async fn get(&self) -> T {
        let value = self.inner.lock().await;
        *value
    }

    pub async fn set(&self, val: T) {
        let mut value = self.inner.lock().await;
        *value = val
    }
}

impl Global<bool> {
    pub async fn toggle(&self) -> bool {
        let mut value = self.inner.lock().await;
        *value = !*value;
        *value
    }
}

impl<T: Sized + Copy + Default> Default for Global<T> {
    fn default() -> Self {
        Global {
            inner: Mutex::new(T::default()),
        }
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

#[derive(Debug)]
pub enum AppError {
    DeserializeFailed,
}

#[derive(Clone, Copy)]
pub struct App<const N: usize> {
    pub app_id: u8,
    pub start_channel: usize,
    max_sender: MaxSender,
    midi_sender: MidiSender,
    event_pubsub: &'static EventPubSubChannel,
}

impl<const N: usize> App<N> {
    pub fn new(
        app_id: u8,
        start_channel: usize,
        max_sender: MaxSender,
        midi_sender: MidiSender,
        event_pubsub: &'static EventPubSubChannel,
    ) -> Self {
        Self {
            app_id,
            start_channel,
            max_sender,
            midi_sender,
            event_pubsub,
        }
    }

    // TODO: We should also probably make sure that people do not reconfigure the jacks within the
    // app (throw error or something)
    async fn reconfigure_jack(&self, chan: usize, config: MaxConfig) {
        self.max_sender
            .send((self.start_channel + chan, MaxCmd::ConfigurePort(config)))
            .await;
    }

    pub fn make_global<T: Sized + Copy>(&self, initial: T) -> Global<T> {
        Global::new(initial)
    }

    // TODO: How can we prevent people from doing this multiple times?
    pub async fn make_in_jack(&self, chan: usize, range: Range) -> InJack {
        let chan = chan.clamp(0, N - 1);
        let adc_range = match range {
            Range::_Neg5_5V => ADCRANGE::RgNeg5_5v,
            _ => ADCRANGE::Rg0_10v,
        };
        self.reconfigure_jack(
            chan,
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
        let chan = chan.clamp(0, N - 1);
        let dac_range = match range {
            Range::_Neg5_5V => DACRANGE::RgNeg5_5v,
            _ => DACRANGE::Rg0_10v,
        };
        self.reconfigure_jack(chan, MaxConfig::Mode5(ConfigMode5(dac_range)))
            .await;

        OutJack::new(self.start_channel + chan, range)
    }

    pub async fn make_gate_jack(&self, chan: usize, level: u16) -> GateJack {
        let chan = chan.clamp(0, N - 1);
        self.reconfigure_jack(chan, MaxConfig::Mode3(ConfigMode3, level))
            .await;

        GateJack::new(self.start_channel + chan, self.max_sender)
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

    pub fn use_buttons(&self) -> Buttons<N> {
        Buttons::new(self.start_channel, self.event_pubsub)
    }

    pub fn use_faders(&self) -> Faders<N> {
        Faders::new(self.start_channel, self.event_pubsub)
    }

    pub fn use_leds(&self) -> Leds<N> {
        Leds::new(self.start_channel)
    }

    pub fn use_die(&self) -> Die {
        Die { rng: RoscRng }
    }

    // TODO: Make sure we can only create one clock per app
    pub fn use_clock(&self) -> Clock {
        Clock::new()
    }

    pub fn use_midi(&self, midi_channel: u8) -> Midi<N> {
        Midi::new(midi_channel.into(), self.midi_sender)
    }

    pub async fn wait_for_scene_event(&self) -> SceneEvent {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            match subscriber.next_message_pure().await {
                InputEvent::LoadScene(scene) => {
                    return SceneEvent::LoadSscene(scene);
                }
                InputEvent::SaveScene(scene) => {
                    return SceneEvent::SaveScene(scene);
                }
                _ => {}
            }
        }
    }

    async fn reset(&self) {
        let leds = self.use_leds();
        leds.reset_all();
        for chan in 0..N {
            self.reconfigure_jack(chan, MaxConfig::Mode0).await;
        }
    }

    pub async fn exit_handler(&self, exit_signal: &'static Signal<NoopRawMutex, bool>) {
        exit_signal.wait().await;
        self.reset().await;
    }
}
