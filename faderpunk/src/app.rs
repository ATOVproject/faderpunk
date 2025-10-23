use core::cell::RefCell;

use embassy_rp::clocks::RoscRng;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use embassy_time::Timer;
use max11300::config::{
    ConfigMode0, ConfigMode3, ConfigMode5, ConfigMode7, Mode, ADCRANGE, AVR, DACRANGE, NSAMPLES,
};
use midly::{live::LiveEvent, num::u4, MidiMessage, PitchBend};
use portable_atomic::Ordering;

use libfp::{
    latch::AnalogLatch,
    quantizer::{Pitch, QuantizerState},
    utils::scale_bits_12_7,
    Brightness, ClockDivision, Color, Range,
};

use crate::{
    events::{EventPubSubChannel, EventPubSubSubscriber, InputEvent},
    tasks::{
        buttons::{is_channel_button_pressed, is_shift_button_pressed},
        clock::{ClockSubscriber, CLOCK_PUBSUB},
        i2c::{I2cLeaderMessage, I2cLeaderSender, I2C_CONNECTED},
        leds::{set_led_mode, LedMode, LedMsg},
        max::{
            MaxCmd, MaxSender, MAX_TRIGGERS_GPO, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER,
        },
        midi::AppMidiSender,
    },
    QUANTIZER,
};

pub use crate::{
    storage::{AppParams, AppStorage, Arr, ManagedStorage, ParamStore},
    tasks::{clock::ClockEvent, leds::Led},
};

#[derive(Clone, Copy)]
pub struct Leds<const N: usize> {
    start_channel: usize,
}

impl<const N: usize> Leds<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
    }

    pub fn set(&self, chan: usize, position: Led, color: Color, brightness: Brightness) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(
            channel,
            position,
            LedMsg::Set(LedMode::Static(color, brightness)),
        );
    }
    pub fn set_mode(&self, chan: usize, position: Led, mode: LedMode) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(channel, position, LedMsg::Set(mode));
    }

    pub fn unset(&self, chan: usize, position: Led) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        set_led_mode(channel, position, LedMsg::Reset);
    }

    pub fn unset_chan(&self, chan: usize) {
        let channel = self.start_channel + chan.clamp(0, N - 1);
        for position in [Led::Top, Led::Bottom, Led::Button] {
            set_led_mode(channel, position, LedMsg::Reset);
        }
    }

    pub fn unset_all(&self) {
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
}

impl GateJack {
    fn new(channel: usize) -> Self {
        Self { channel }
    }

    pub async fn set_high(&self) {
        MAX_TRIGGERS_GPO[self.channel].store(2, Ordering::Relaxed);
    }

    pub async fn set_low(&self) {
        MAX_TRIGGERS_GPO[self.channel].store(1, Ordering::Relaxed);
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

    /// Returns the number of the button that was released
    pub async fn wait_for_any_up(&self) -> (usize, bool) {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::ButtonUp(channel) = subscriber.next_message_pure().await {
                if (self.start_channel..self.start_channel + N).contains(&channel) {
                    return (channel - self.start_channel, self.is_shift_pressed());
                }
            }
        }
    }

    /// Returns if shift was pressed during button up
    pub async fn wait_for_up(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let (channel, is_shift_pressed) = self.wait_for_any_up().await;
            if chan == channel {
                return is_shift_pressed;
            }
        }
    }

    pub async fn wait_for_any_long_press(&self) -> (usize, bool) {
        let mut subscriber = self.event_pubsub.subscriber().unwrap();

        loop {
            if let InputEvent::ButtonLongPress(channel) = subscriber.next_message_pure().await {
                if (self.start_channel..self.start_channel + N).contains(&channel) {
                    return (channel - self.start_channel, self.is_shift_pressed());
                }
            }
        }
    }

    pub async fn wait_for_long_press(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        loop {
            let (channel, is_shift_pressed) = self.wait_for_any_long_press().await;
            if chan == channel {
                return is_shift_pressed;
            }
        }
    }

    pub fn is_button_pressed(&self, chan: usize) -> bool {
        let chan = chan.clamp(0, N - 1);
        is_channel_button_pressed(self.start_channel + chan)
    }

    pub fn is_shift_pressed(&self) -> bool {
        is_shift_button_pressed()
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
    tick_count: u16,
}

impl Clock {
    pub fn new() -> Self {
        let subscriber = CLOCK_PUBSUB.subscriber().unwrap();
        Self {
            subscriber,
            tick_count: 0,
        }
    }

    pub async fn wait_for_event(&mut self, division: ClockDivision) -> ClockEvent {
        loop {
            match self.subscriber.next_message_pure().await {
                ClockEvent::Tick => {
                    self.tick_count += 1;
                    if self.tick_count >= division as u16 {
                        self.tick_count = 0;
                        return ClockEvent::Tick;
                    }
                }
                ClockEvent::Stop => {
                    return ClockEvent::Stop;
                }
                clock_event @ ClockEvent::Start | clock_event @ ClockEvent::Reset => {
                    self.tick_count = 0;
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
pub struct I2cOutput<const N: usize> {
    i2c_sender: I2cLeaderSender,
    start_channel: usize,
}

impl<const N: usize> I2cOutput<N> {
    pub fn new(start_channel: usize, i2c_sender: I2cLeaderSender) -> Self {
        Self {
            i2c_sender,
            start_channel,
        }
    }

    pub async fn send_fader_value(&self, chan: usize, value: u16) {
        if I2C_CONNECTED.load(Ordering::Relaxed) {
            let chan = chan.clamp(0, N - 1);
            let msg = I2cLeaderMessage::FaderValue(self.start_channel + chan, value);
            self.i2c_sender.send(msg).await;
        }
    }
}

#[derive(Clone, Copy)]
pub struct MidiOutput {
    start_channel: usize,
    midi_sender: AppMidiSender,
    midi_channel: u4,
}

impl MidiOutput {
    pub fn new(start_channel: usize, midi_channel: u4, midi_sender: AppMidiSender) -> Self {
        Self {
            start_channel,
            midi_sender,
            midi_channel,
        }
    }

    async fn send_midi_msg(&self, msg: MidiMessage) {
        let event = LiveEvent::Midi {
            channel: self.midi_channel,
            message: msg,
        };
        self.midi_sender.send((self.start_channel, event)).await;
    }

    /// Sends a MIDI CC message.
    /// value is normalized to a range of 0-4095
    pub async fn send_cc(&self, cc: u8, value: u16) {
        let msg = MidiMessage::Controller {
            controller: cc.into(),
            value: scale_bits_12_7(value),
        };
        self.send_midi_msg(msg).await;
    }

    /// Sends a MIDI NoteOn message.
    /// velocity is normalized to a range of 0-4095
    pub async fn send_note_on(&self, note_number: u8, velocity: u16) {
        let msg = MidiMessage::NoteOn {
            key: note_number.into(),

            vel: scale_bits_12_7(velocity),
        };
        self.send_midi_msg(msg).await;
    }

    /// Sends a MIDI NoteOff message.
    pub async fn send_note_off(&self, note_number: u8) {
        let msg = MidiMessage::NoteOff {
            key: note_number.into(),
            vel: 0.into(),
        };
        self.send_midi_msg(msg).await;
    }

    /// Sends a MIDI Aftertouch message.
    /// velocity is normalized to a range of 0-4095
    pub async fn send_aftertouch(&self, note_number: u8, velocity: u16) {
        let msg = MidiMessage::Aftertouch {
            key: note_number.into(),
            vel: scale_bits_12_7(velocity),
        };
        self.send_midi_msg(msg).await;
    }

    /// Sends a MIDI PitchBend message.
    /// bend is a value between 0 and 16,383
    pub async fn send_pitch_bend(&self, bend: u16) {
        let msg = MidiMessage::PitchBend {
            bend: PitchBend(bend.into()),
        };
        self.send_midi_msg(msg).await;
    }
}

pub struct MidiInput {
    subscriber: EventPubSubSubscriber,
    midi_channel: u4,
}

impl MidiInput {
    pub fn new(midi_channel: u4, event_pubsub: &'static EventPubSubChannel) -> Self {
        let subscriber = event_pubsub.subscriber().unwrap();
        Self {
            subscriber,
            midi_channel,
        }
    }

    pub async fn wait_for_message(&mut self) -> MidiMessage {
        loop {
            if let InputEvent::MidiMsg(LiveEvent::Midi { channel, message }) =
                self.subscriber.next_message_pure().await
            {
                if channel == self.midi_channel {
                    return message;
                }
            }
        }
    }
}

pub struct Global<T: Sized> {
    inner: RefCell<T>,
}

impl<T: Sized + Copy> Global<T> {
    pub fn new(initial: T) -> Self {
        Self {
            inner: RefCell::new(initial),
        }
    }

    pub fn get(&self) -> T {
        let value = self.inner.borrow();
        *value
    }

    pub fn set(&self, val: T) -> T {
        let mut value = self.inner.borrow_mut();
        *value = val;
        *value
    }

    pub fn modify<F>(&self, modifier: F) -> T
    where
        F: FnOnce(&T) -> T,
    {
        let mut guard = self.inner.borrow_mut();
        *guard = modifier(&*guard);
        *guard
    }
}

impl Global<bool> {
    pub fn toggle(&self) -> bool {
        let mut value = self.inner.borrow_mut();
        *value = !*value;
        *value
    }
}

impl<T: Sized + Copy + Default> Default for Global<T> {
    fn default() -> Self {
        Global {
            inner: RefCell::new(T::default()),
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

pub struct Quantizer {
    range: Range,
    state: RefCell<QuantizerState>,
}

impl Quantizer {
    pub fn new(range: Range) -> Self {
        Self {
            range,
            state: RefCell::new(QuantizerState::default()),
        }
    }
    /// Quantize a note
    pub async fn get_quantized_note(&self, value: u16) -> Pitch {
        let value = value.clamp(0, 4095);
        let quantizer = QUANTIZER.get().lock().await;
        let mut state = self.state.borrow_mut();
        quantizer.get_quantized_note(&mut state, value, self.range)
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
    pub layout_id: u8,
    event_pubsub: &'static EventPubSubChannel,
    i2c_sender: I2cLeaderSender,
    max_sender: MaxSender,
    midi_sender: AppMidiSender,
}

impl<const N: usize> App<N> {
    pub fn new(
        app_id: u8,
        start_channel: usize,
        layout_id: u8,
        event_pubsub: &'static EventPubSubChannel,
        i2c_sender: I2cLeaderSender,
        max_sender: MaxSender,
        midi_sender: AppMidiSender,
    ) -> Self {
        Self {
            app_id,
            event_pubsub,
            i2c_sender,
            layout_id,
            max_sender,
            midi_sender,
            start_channel,
        }
    }

    async fn reconfigure_jack(&self, chan: usize, mode: Mode, gpo_level: Option<u16>) {
        self.max_sender
            .send((
                self.start_channel + chan,
                MaxCmd::ConfigurePort(mode, gpo_level),
            ))
            .await;
    }

    pub fn make_global<T: Sized + Copy>(&self, initial: T) -> Global<T> {
        Global::new(initial)
    }

    pub fn make_latch(&self, initial: u16) -> AnalogLatch {
        AnalogLatch::new(initial)
    }

    pub async fn make_in_jack(&self, chan: usize, range: Range) -> InJack {
        let chan = chan.clamp(0, N - 1);
        let adc_range = match range {
            Range::_Neg5_5V => ADCRANGE::RgNeg5_5v,
            _ => ADCRANGE::Rg0_10v,
        };
        self.reconfigure_jack(
            chan,
            Mode::Mode7(ConfigMode7(AVR::InternalRef, adc_range, NSAMPLES::Samples1)),
            None,
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
        self.reconfigure_jack(chan, Mode::Mode5(ConfigMode5(dac_range)), None)
            .await;

        OutJack::new(self.start_channel + chan, range)
    }

    pub async fn make_gate_jack(&self, chan: usize, level: u16) -> GateJack {
        let chan = chan.clamp(0, N - 1);
        self.reconfigure_jack(chan, Mode::Mode3(ConfigMode3), Some(level))
            .await;

        GateJack::new(self.start_channel + chan)
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

    pub fn use_quantizer(&self, range: Range) -> Quantizer {
        Quantizer::new(range)
    }

    pub fn use_midi_input(&self, midi_channel: u8) -> MidiInput {
        MidiInput::new(midi_channel.into(), self.event_pubsub)
    }

    pub fn use_midi_output(&self, midi_channel: u8) -> MidiOutput {
        MidiOutput::new(self.start_channel, midi_channel.into(), self.midi_sender)
    }

    pub fn use_i2c_output(&self) -> I2cOutput<N> {
        I2cOutput::new(self.start_channel, self.i2c_sender)
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
        leds.unset_all();
        for chan in 0..N {
            self.reconfigure_jack(chan, Mode::Mode0(ConfigMode0), None)
                .await;
        }
    }

    pub async fn exit_handler(&self, exit_signal: &'static Signal<NoopRawMutex, bool>) {
        exit_signal.wait().await;
        self.reset().await;
    }
}
