use defmt::info;
use embassy_rp::clocks::RoscRng;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Sender,
    mutex::Mutex,
    signal::Signal,
    watch::Receiver,
};
use embassy_time::{with_timeout, Duration, Timer};
use heapless::Vec;
use max11300::{
    config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES},
    ConfigurePort,
};
use midly::{live::LiveEvent, num::u4, MidiMessage};
use portable_atomic::Ordering;
use postcard::{from_bytes, to_slice};
use rand::Rng;

use config::Curve;
use libfp::{
    constants::{CHAN_LED_MAP, CURVE_EXP, CURVE_LOG},
    utils::scale_bits_12_7,
};
use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};

use crate::{
    scene::get_scene,
    tasks::{
        buttons::BUTTON_PRESSED,
        leds::LED_VALUES,
        max::{MaxCmd, MaxConfig, MAX_VALUES_ADC, MAX_VALUES_DAC, MAX_VALUES_FADER},
    },
    CmdSender, EventPubSubChannel, HardwareCmd, HardwareEvent, CLOCK_WATCH,
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

#[derive(Clone, Copy)]
pub struct Arr<T: Sized + Copy + Default, const N: usize>(pub [T; N]);

impl<T: Sized + Copy + Default, const N: usize> Default for Arr<T, N> {
    fn default() -> Self {
        Self([T::default(); N])
    }
}

impl<T, const N: usize> Serialize for Arr<T, N>
where
    T: Serialize + Sized + Copy + Default,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let vec = Vec::<T, N>::from_slice(&self.0).unwrap();
        vec.serialize(serializer)
    }
}

impl<'de, T, const N: usize> Deserialize<'de> for Arr<T, N>
where
    T: Deserialize<'de> + Sized + Copy + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec = Vec::<T, N>::deserialize(deserializer)?;
        if vec.len() != N {
            return Err(D::Error::invalid_length(
                vec.len(),
                &"an array of exact length N",
            ));
        }
        let mut arr = [T::default(); N];
        arr.copy_from_slice(vec.as_slice()); // Safe due to length check above
        Ok(Arr(arr))
    }
}

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
    cmd_sender: CmdSender,
}

impl GateJack {
    fn new(channel: usize, cmd_sender: CmdSender) -> Self {
        Self {
            channel,
            cmd_sender,
        }
    }

    pub async fn set_high(&self) {
        self.cmd_sender
            .send(HardwareCmd::MaxCmd(self.channel, MaxCmd::GpoSetHigh))
            .await;
    }

    pub async fn set_low(&self) {
        self.cmd_sender
            .send(HardwareCmd::MaxCmd(self.channel, MaxCmd::GpoSetLow))
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
            if let HardwareEvent::ButtonDown(channel) = subscriber.next_message_pure().await {
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
            if let HardwareEvent::FaderChange(channel) = subscriber.next_message_pure().await {
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
    receiver: Receiver<'static, CriticalSectionRawMutex, bool, 16>,
}

impl Clock {
    pub fn new() -> Self {
        let receiver = CLOCK_WATCH.receiver().unwrap();
        Self { receiver }
    }

    // TODO: division needs to be an enum
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

pub struct Midi<const N: usize> {
    cmd_sender: CmdSender,
    midi_channel: u4,
}

impl<const N: usize> Midi<N> {
    pub fn new(midi_channel: u4, cmd_sender: CmdSender) -> Self {
        Self {
            cmd_sender,
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
        self.cmd_sender.send(HardwareCmd::MidiMsg(msg)).await;
    }
}

pub struct Global<T: Sized + Copy> {
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

pub struct Die {
    rng: RoscRng,
}

impl Die {
    /// Returns a random number between 0 and 4095
    pub fn roll(&mut self) -> u16 {
        self.rng.gen_range(0..=4095)
    }
}

pub struct App<'a, const N: usize> {
    app_id: u8,
    pub start_channel: usize,
    channel_count: usize,
    cmd_sender: CmdSender,
    event_pubsub: &'static EventPubSubChannel,
    scene_signal: &'a Signal<NoopRawMutex, u8>,
}

impl<'a, const N: usize> App<'a, N> {
    pub fn new(
        app_id: u8,
        start_channel: usize,
        cmd_sender: CmdSender,
        event_pubsub: &'static EventPubSubChannel,
        scene_signal: &'a Signal<NoopRawMutex, u8>,
    ) -> Self {
        Self {
            app_id,
            start_channel,
            channel_count: N,
            cmd_sender,
            event_pubsub,
            scene_signal,
        }
    }

    // TODO: We should also probably make sure that people do not reconfigure the jacks within the
    // app (throw error or something)
    async fn reconfigure_jack(&self, channel: usize, config: MaxConfig) {
        self.cmd_sender
            .send(HardwareCmd::MaxCmd(channel, MaxCmd::ConfigurePort(config)))
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
        let chan = chan.clamp(0, N - 1);
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
        let chan = chan.clamp(0, N - 1);
        self.reconfigure_jack(
            self.start_channel + chan,
            MaxConfig::Mode3(ConfigMode3, level),
        )
        .await;

        GateJack::new(self.start_channel + chan, self.cmd_sender)
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

    pub async fn wait_for_scene_change(&self) -> u8 {
        return self.scene_signal.wait().await;
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

    pub fn use_clock(&self) -> Clock {
        Clock::new()
    }

    pub fn use_midi(&self, midi_channel: u8) -> Midi<N> {
        Midi::new(midi_channel.into(), self.cmd_sender)
    }
}
