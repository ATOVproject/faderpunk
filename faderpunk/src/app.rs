use embassy_rp::clocks::RoscRng;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer};
use max11300::config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midly::{live::LiveEvent, num::u4, MidiMessage};
use portable_atomic::Ordering;

use libfp::{ext::BrightnessExt, utils::scale_bits_12_7};

use crate::{
    events::{EventPubSubChannel, InputEvent},
    tasks::{
        buttons::BUTTON_PRESSED,
        clock::{ClockSubscriber, CLOCK_PUBSUB},
        leds::{set_led_mode, LedMode, LedMsg},
        max::{MaxCmd, MaxConfig, MaxSender, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
        midi::MidiSender,
    },
};

pub use crate::{
    storage::{AppStorage, Arr, ManagedStorage, ParamSlot, ParamStore},
    tasks::{clock::ClockEvent, leds::Led},
};
pub use smart_leds::{colors, RGB8};

pub enum Range {
    // 0 - 10V
    _0_10V,
    // 0 - 5V
    _0_5V,
    // -5 - 5V
    _Neg5_5V,
}

#[derive(Clone, Copy)]
pub struct Leds<const N: usize> {
    start_channel: usize,
}

impl<const N: usize> Leds<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }

    pub fn set(&self, chan: usize, position: Led, color: RGB8, brightness: u8) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(
            channel,
            position,
            LedMsg::Set(LedMode::Static(color.scale(brightness))),
        );
    }
    pub fn set_mode(&self, chan: usize, position: Led, mode: LedMode) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(channel, position, LedMsg::Set(mode));
    }

    pub fn reset(&self, chan: usize, position: Led) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(channel, position, LedMsg::Reset);
    }

    pub fn reset_chan(&self, chan: usize) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        for position in [Led::Top, Led::Bottom, Led::Button] {
            set_led_mode(channel, position, LedMsg::Reset);
        }
    }

    pub fn reset_all(&self) {
        for chan in 0..N {
            let channel = self.start_channel + chan.clamp(0, N - 1);
            for position in [Led::Top, Led::Bottom, Led::Button] {
                set_led_mode(channel, position, LedMsg::Reset);
            }
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
    pub async fn wait_for_any_down(&self) -> (usize, bool) {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::ButtonDown(channel) = subscriber.next_message_pure().await {
                if (self.start_channel..self.start_channel + N).contains(&channel) {
                    return (channel - self.start_channel, self.is_shift_pressed());
                }
            }
        }
    }

    /// Returns if shift was pressed during button down
    pub async fn wait_for_down(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let (channel, is_shift_pressed) = self.wait_for_any_down().await;
            if chan == channel {
                return is_shift_pressed;
            }
        }
    }

    pub async fn wait_for_any_long_press(&self, duration: Duration) -> (usize, bool) {
        loop {
            let (channel, is_shift_pressed) = self.wait_for_any_down().await;
            Timer::after(duration).await;
            if BUTTON_PRESSED[self.start_channel + channel].load(Ordering::Relaxed) {
                return (channel, is_shift_pressed);
            }
        }
    }

    pub async fn wait_for_long_press(&self, chan: usize, duration: Duration) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let (channel, is_shift_pressed) = self.wait_for_any_long_press(duration).await;
            if chan == channel {
                return is_shift_pressed;
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

    pub async fn wait_for_change_at(&self, chan: usize) {
        let chan = chan.clamp(0, N - 1);
        loop {
            let channel = self.wait_for_any_change().await;
            if chan == channel {
                return;
            }
        }
    }

    pub fn get_value_at(&self, chan: usize) -> u16 {
        let chan = chan.clamp(0, N - 1);
        MAX_VALUES_FADER[self.start_channel + chan].load(Ordering::Relaxed)
    }

    pub fn get_all_values(&self) -> [u16; N] {
        let mut buf = [0_u16; N];
        for i in 0..N {
            buf[i] = MAX_VALUES_FADER[self.start_channel + i].load(Ordering::Relaxed);
        }
        buf
    }
}

impl Faders<1> {
    pub fn get_value(&self) -> u16 {
        MAX_VALUES_FADER[self.start_channel].load(Ordering::Relaxed)
    }

    pub async fn wait_for_change(&self) {
        self.wait_for_any_change().await;
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
    event_pubsub: &'static EventPubSubChannel,
    midi_sender: MidiSender,
    midi_channel: u4,
}

impl<const N: usize> Midi<N> {
    pub fn new(
        midi_channel: u4,
        midi_sender: MidiSender,
        event_pubsub: &'static EventPubSubChannel,
    ) -> Self {
        Self {
            event_pubsub,
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

    pub async fn wait_for_message(&self) -> LiveEvent<'static> {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::MidiMsg(msg) = subscriber.next_message_pure().await {
                return msg;
            }
        }
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

    pub async fn get(&self) -> T {
        let value = self.inner.lock().await;
        *value
    }

    pub async fn set(&self, val: T) {
        let mut value = self.inner.lock().await;
        *value = val
    }

    pub async fn modify<F>(&self, modifier: F) -> T
    where
        F: FnOnce(&mut T) -> T,
    {
        let mut value = self.inner.lock().await;
        modifier(&mut *value)
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

#[derive(Clone, Copy)]
pub struct Die;

impl Die {
    pub fn new() -> Self {
        Self
    }
    /// Returns a random number between 0 and 4095.
    pub fn roll(&self) -> u16 {
        let b1 = RoscRng::next_u8();
        let b2 = RoscRng::next_u8();
        let random_u16 = u16::from_le_bytes([b1, b2]);
        random_u16 % 4096
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
    event_pubsub: &'static EventPubSubChannel,
    max_sender: MaxSender,
    midi_sender: MidiSender,
}

impl<const N: usize> App<N> {
    pub fn new(
        app_id: u8,
        start_channel: usize,
        event_pubsub: &'static EventPubSubChannel,
        max_sender: MaxSender,
        midi_sender: MidiSender,
    ) -> Self {
        Self {
            app_id,
            start_channel,
            max_sender,
            midi_sender,
            event_pubsub,
        }
    }

    async fn reconfigure_jack(&self, chan: usize, config: MaxConfig) {
        self.max_sender
            .send((self.start_channel + chan, MaxCmd::ConfigurePort(config)))
            .await;
    }

    pub fn make_global<T: Sized + Copy>(&self, initial: T) -> Global<T> {
        Global::new(initial)
    }

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
        Die::new()
    }

    pub fn use_clock(&self) -> Clock {
        Clock::new()
    }

    pub fn use_midi(&self, midi_channel: u8) -> Midi<N> {
        Midi::new(midi_channel.into(), self.midi_sender, self.event_pubsub)
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
