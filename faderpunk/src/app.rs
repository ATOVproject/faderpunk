use embassy_rp::clocks::RoscRng;
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, ThreadModeRawMutex},
    channel::Sender,
    mutex::Mutex,
    pubsub::{PubSubChannel, Subscriber},
    watch::Receiver,
};
use embassy_time::{with_timeout, Duration, Instant, TimeoutError, Timer};
use heapless::Vec;
use max11300::config::{ConfigMode3, ConfigMode5, ConfigMode7, ADCRANGE, AVR, DACRANGE, NSAMPLES};
use midly::{
    live::LiveEvent,
    num::{u4, u7},
    MidiMessage,
};
use portable_atomic::Ordering;
use postcard::{from_bytes, to_slice, to_vec};
use rand::Rng;

use config::{Config, Curve, Value, Waveform};
use libfp::{
    constants::{CHAN_LED_MAP, CURVE_EXP, CURVE_LOG},
    utils::scale_bits_12_7,
};
use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};

use crate::{
    tasks::{
        buttons::BUTTON_PRESSED,
        eeprom::{StorageMsg, DATA_LENGTH},
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

pub struct AppConfig<const N: usize> {
    values: [Value; N],
}

impl<const N: usize> AppConfig<N> {
    pub fn from(config: &Config<N>) -> Self {
        let values = config.get_default_values();
        Self { values }
    }

    fn value(&self, index: usize) -> Option<&Value> {
        if index < self.values.len() {
            Some(&self.values[index])
        } else {
            None
        }
    }

    pub fn get_int_at(&self, index: usize) -> i32 {
        match self.value(index) {
            Some(Value::Int(val)) => *val,
            _ => 0,
        }
    }

    pub fn get_float_at(&self, index: usize) -> f32 {
        match self.value(index) {
            Some(Value::Float(val)) => *val,
            _ => 0.0,
        }
    }

    pub fn get_bool_at(&self, index: usize) -> bool {
        match self.value(index) {
            Some(Value::Bool(val)) => *val,
            _ => false,
        }
    }

    pub fn get_enum_at(&self, index: usize) -> usize {
        match self.value(index) {
            Some(Value::Enum(val)) => *val,
            _ => 0,
        }
    }

    pub fn get_curve_at(&self, index: usize) -> Curve {
        match self.value(index) {
            Some(Value::Curve(val)) => *val,
            _ => Curve::Linear,
        }
    }

    pub fn get_waveform_at(&self, index: usize) -> Waveform {
        match self.value(index) {
            Some(Value::Waveform(val)) => *val,
            _ => Waveform::Sine,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum StorageSlot {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
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
    start_channel: usize,
}

impl<const N: usize> Faders<N> {
    pub fn new(start_channel: usize) -> Self {
        Self { start_channel }
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
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
}

impl Clock {
    pub fn new(sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>) -> Self {
        let receiver = CLOCK_WATCH.receiver().unwrap();
        Self { receiver, sender }
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

    //TODO: Check if app is CLOCK app and if not, do not implement this
    //HINT: Can we use struct markers? Or how to do it?
    pub async fn set_bpm(&self, bpm: f32) {
        self.sender.send((16, XRxMsg::SetBpm(bpm))).await;
    }
}

pub struct Midi<const N: usize> {
    midi_channel: u4,
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
    start_channel: usize,
}

impl<const N: usize> Midi<N> {
    pub fn new(
        start_channel: usize,
        midi_channel: u4,
        sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
    ) -> Self {
        Self {
            midi_channel,
            sender,
            start_channel,
        }
    }
    // TODO: This is a short-hand function that should also send the msg via TRS
    // Create and use a function called midi_send_both and use it here
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

    // TODO: Check if making midi an own struct with a listener would make sense
    pub async fn send_msg(&self, msg: LiveEvent<'static>) {
        self.sender
            .send((self.start_channel, XRxMsg::MidiMessage(msg)))
            .await;
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

pub struct GlobalWithStorage<T: Sized + Copy + Default> {
    app_id: u8,
    inner: Global<T>,
    start_channel: u8,
    storage_slot: u8,
    sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
}

impl<T: Sized + Copy + Default + Serialize + DeserializeOwned> GlobalWithStorage<T> {
    pub fn new(
        app_id: u8,
        initial: T,
        start_channel: u8,
        storage_slot: StorageSlot,
        sender: Sender<'static, NoopRawMutex, (usize, XRxMsg), 128>,
    ) -> Self {
        Self {
            app_id,
            start_channel,
            inner: Global::new(initial),
            storage_slot: storage_slot as u8,
            sender,
        }
    }

    async fn ser(&self) -> Vec<u8, DATA_LENGTH> {
        let value = self.get().await;
        let mut buf: [u8; DATA_LENGTH] = [0; DATA_LENGTH];
        let serialized = to_slice(&value, &mut buf).unwrap();
        Vec::<u8, DATA_LENGTH>::from_slice(serialized).unwrap()
    }

    async fn des(&mut self, data: &[u8]) {
        if let Ok(val) = from_bytes::<T>(data) {
            self.set(val).await;
        }
    }

    pub async fn get(&self) -> T {
        self.inner.get().await
    }

    pub async fn set(&mut self, val: T) {
        self.inner.set(val).await
    }

    pub async fn save(&self) {
        let ser = self.ser().await;
        self.sender
            .send((
                self.start_channel as usize,
                XRxMsg::StorageMsg(StorageMsg::Store(self.app_id, self.storage_slot, ser)),
            ))
            .await;
    }

    pub async fn load(&mut self) {
        self.sender
            .send((
                self.start_channel as usize,
                XRxMsg::StorageMsg(StorageMsg::Request(self.app_id, self.storage_slot)),
            ))
            .await;
        // Make this timeout roughly as long as the boot sequence ;)
        with_timeout(Duration::from_millis(2000), async {
            let mut subscriber = CHANS_X[self.start_channel as usize].subscriber().unwrap();
            loop {
                if let (_, XTxMsg::StorageMsg(StorageMsg::Read(app_id, storage_slot, res))) =
                    subscriber.next_message_pure().await
                {
                    if self.app_id == app_id && self.storage_slot == storage_slot {
                        self.des(res.as_slice()).await;
                        return;
                    }
                }
            }
        })
        .await
        .ok();
    }
}

impl GlobalWithStorage<bool> {
    pub async fn toggle(&self) -> bool {
        self.inner.toggle().await
    }
}

impl<T: Sized + Copy + Default, const N: usize> GlobalWithStorage<Arr<T, N>> {
    pub async fn set_array(&self, val: [T; N]) {
        self.inner.set(Arr(val)).await
    }

    pub async fn get_array(&self) -> [T; N] {
        self.inner.get().await.0
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
    pub start_channel: usize,
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

    pub fn make_global_with_store<T: Sized + Copy + Default + Serialize + DeserializeOwned>(
        &self,
        initial: T,
        storage_slot: StorageSlot,
    ) -> GlobalWithStorage<T> {
        GlobalWithStorage::new(
            self.app_id as u8,
            initial,
            self.start_channel as u8,
            storage_slot,
            self.sender,
        )
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

    pub fn use_buttons(&self) -> Buttons<N> {
        Buttons::new(self.start_channel)
    }

    pub fn use_faders(&self) -> Faders<N> {
        Faders::new(self.start_channel)
    }

    pub fn use_leds(&self) -> Leds<N> {
        Leds::new(self.start_channel)
    }

    pub fn use_die(&self) -> Die {
        Die { rng: RoscRng }
    }

    pub fn use_clock(&self) -> Clock {
        Clock::new(self.sender)
    }

    pub fn use_midi(&self, midi_channel: u8) -> Midi<N> {
        Midi::new(self.start_channel, midi_channel.into(), self.sender)
    }
}
