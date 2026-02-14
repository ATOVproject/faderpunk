#![no_std]

use core::ops::Add;

use embassy_time::Duration;
use heapless::Vec;
use max11300::config::{ADCRANGE, DACRANGE};
use midly::num::{u4, u7};
use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

pub mod colors;
pub mod constants;
pub mod ext;
pub mod i2c_proto;
pub mod latch;
pub mod quantizer;
pub mod types;
pub mod utils;
pub mod fp_grids_lib;

// Re-export commonly used latch types
pub use latch::{AnalogLatch, LatchLayer, TakeoverMode};

use constants::{
    CURVE_EXP, CURVE_LOG, WAVEFORM_SAW, WAVEFORM_SAW_INV, WAVEFORM_SINE, WAVEFORM_SQUARE,
    WAVEFORM_TRIANGLE,
};
use smart_leds::RGB8;

use crate::ext::FromValue;
use colors::{
    BLUE, CYAN, GREEN, LIGHT_BLUE, LIME, ORANGE, PALE_GREEN, PINK, RED, ROSE, SALMON, SAND,
    SKY_BLUE, VIOLET, WHITE, YELLOW,
};

/// Total channel size of this device
pub const GLOBAL_CHANNELS: usize = 16;

/// The devices I2C address (as a follower)
pub const I2C_ADDRESS: u16 = 0x56;
pub const I2C_ADDRESS_CALIBRATION: u16 = 0x57;

/// Maximum number of params per app
pub const APP_MAX_PARAMS: usize = 16;

/// Length of the startup animation
pub const STARTUP_ANIMATION_DURATION: Duration = Duration::from_secs(2);

/// Range in which the LED brightness is scaled
pub const LED_BRIGHTNESS_RANGE: core::ops::Range<u8> = 100..255;

pub const CALIBRATION_SCALE_FACTOR: i64 = 1 << 16;
pub const CALIBRATION_VERSION_LATEST: u8 = 2;
pub const CALIB_FILE_MAGIC: [u8; 4] = *b"FPBC";

pub type ConfigMeta<'a> = (usize, &'a str, &'a str, Color, AppIcon, &'a [Param]);

/// The config layout is a layout with all the apps in the appropriate spots
// (app_id, channels, layout_id)
pub type InnerLayout = [Option<(u8, usize, u8)>; GLOBAL_CHANNELS];

#[derive(Clone, Serialize, Deserialize, PostcardBindings)]
pub struct Layout(pub InnerLayout);

impl Layout {
    pub fn validate(&mut self, get_channels: fn(u8) -> Option<usize>) -> bool {
        let mut validated: InnerLayout = [None; GLOBAL_CHANNELS];
        let mut occupied = [false; GLOBAL_CHANNELS];
        let mut used_ids: Vec<u8, { GLOBAL_CHANNELS }> = Vec::new();

        for (app_id, start_channel, _channels, layout_id) in self.iter() {
            let validated_layout_id =
                if used_ids.contains(&layout_id) || layout_id >= GLOBAL_CHANNELS as u8 {
                    // ID is a duplicate or out of bounds, find the next available one
                    (0..GLOBAL_CHANNELS as u8)
                        .find(|id| !used_ids.contains(id))
                        // This is safe because a free slot is guaranteed
                        .unwrap()
                } else {
                    layout_id
                };

            let _ = used_ids.push(validated_layout_id);

            // Re-verify the channel count for the app_id. Skip if it's not a valid app_id
            let Some(channels) = get_channels(app_id) else {
                continue;
            };

            let end_channel = start_channel + channels;

            // Check if the app fits within the channel count and doesn't overlap
            if end_channel <= GLOBAL_CHANNELS
                && !occupied[start_channel..end_channel].iter().any(|&o| o)
            {
                // Mark channels as occupied
                for occ in occupied.iter_mut().take(end_channel).skip(start_channel) {
                    *occ = true;
                }
                // Add the app to the validated layout
                validated[start_channel] = Some((app_id, channels, validated_layout_id));
            }
        }

        let changed = self.0 != validated;
        self.0 = validated;

        changed
    }

    pub fn iter(&self) -> LayoutIter<'_> {
        self.into_iter()
    }

    pub fn count(&self) -> usize {
        self.iter().count()
    }

    pub fn get_layout_ids(&self) -> Vec<u8, { GLOBAL_CHANNELS }> {
        self.iter().map(|(_, _, _, layout_id)| layout_id).collect()
    }
}

impl Default for Layout {
    fn default() -> Self {
        let inner_layout: InnerLayout = core::array::from_fn(|idx| Some((1, 1, idx as u8)));
        Self(inner_layout)
    }
}

pub struct LayoutIter<'a> {
    slice: &'a [Option<(u8, usize, u8)>],
    index: usize,
}

impl Iterator for LayoutIter<'_> {
    // (app_id, start_channel, channels, layout_id)
    type Item = (u8, usize, usize, u8);

    fn next(&mut self) -> Option<Self::Item> {
        // Skip None values
        while self.index < self.slice.len() {
            if let Some(value) = self.slice[self.index] {
                let idx = self.index;
                self.index += 1;
                return Some((value.0, idx, value.1, value.2));
            }
            self.index += 1;
        }
        None
    }
}

impl<'a> IntoIterator for &'a Layout {
    type Item = (u8, usize, usize, u8);
    type IntoIter = LayoutIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        LayoutIter {
            slice: &self.0,
            index: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum ClockSrc {
    None,
    Atom,
    Meteor,
    Cube,
    Internal,
    MidiIn,
    MidiUsb,
}

impl From<ResetSrc> for ClockSrc {
    fn from(value: ResetSrc) -> Self {
        match value {
            ResetSrc::None => ClockSrc::None,
            ResetSrc::Atom => ClockSrc::Atom,
            ResetSrc::Meteor => ClockSrc::Meteor,
            ResetSrc::Cube => ClockSrc::Cube,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum ResetSrc {
    None,
    Atom,
    Meteor,
    Cube,
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum I2cMode {
    Calibration,
    Leader,
    Follower,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum Note {
    #[default]
    C = 0,
    CSharp = 1,
    D = 2,
    DSharp = 3,
    E = 4,
    F = 5,
    FSharp = 6,
    G = 7,
    GSharp = 8,
    A = 9,
    ASharp = 10,
    B = 11,
}

impl From<u8> for Note {
    fn from(value: u8) -> Self {
        match value.min(11) {
            0 => Note::C,
            1 => Note::CSharp,
            2 => Note::D,
            3 => Note::DSharp,
            4 => Note::E,
            5 => Note::F,
            6 => Note::FSharp,
            7 => Note::G,
            8 => Note::GSharp,
            9 => Note::A,
            10 => Note::ASharp,
            11 => Note::B,
            _ => Note::C,
        }
    }
}

impl FromValue for Note {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Note(n) => n,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum Key {
    #[default]
    Chromatic,
    Ionian,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    Locrian,
    BluesMaj,
    BluesMin,
    PentatonicMaj,
    PentatonicMin,
    Folk,
    Japanese,
    Gamelan,
    HungarianMin,
}

impl Key {
    /// Get the u16 bitmask
    pub fn as_u16_key(&self) -> u16 {
        match self {
            Key::Chromatic => 0b111111111111,
            Key::Ionian => 0b101011010101,
            Key::Dorian => 0b101101010110,
            Key::Phrygian => 0b110101011010,
            Key::Lydian => 0b101010110101,
            Key::Mixolydian => 0b101011010110,
            Key::Aeolian => 0b101101011010,
            Key::Locrian => 0b110101101010,
            Key::BluesMaj => 0b101110010100,
            Key::BluesMin => 0b100101110010,
            Key::PentatonicMaj => 0b101010010100,
            Key::PentatonicMin => 0b100101010010,
            Key::Folk => 0b110111011010,
            Key::Japanese => 0b110001011000,
            Key::Gamelan => 0b110100011000,
            Key::HungarianMin => 0b101100111001,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub enum MidiOutMode {
    None,
    Local,
    MidiThru { sources: MidiIn },
    MidiMerge { sources: MidiIn },
}

#[derive(Clone, Copy, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub struct MidiOutConfig {
    pub send_clock: bool,
    pub send_transport: bool,
    pub mode: MidiOutMode,
}

#[allow(clippy::new_without_default)]
impl MidiOutConfig {
    pub const fn new() -> Self {
        Self {
            send_clock: true,
            send_transport: true,
            mode: MidiOutMode::Local,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub struct MidiConfig {
    // [usb, out1, out2]
    pub outs: [MidiOutConfig; 3],
}

#[allow(clippy::new_without_default)]
impl MidiConfig {
    pub const fn new() -> Self {
        Self {
            outs: [MidiOutConfig::new(); 3],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub struct ClockConfig {
    pub clock_src: ClockSrc,
    pub ext_ppqn: u8,
    pub reset_src: ResetSrc,
    pub internal_bpm: f32,
}

#[allow(clippy::new_without_default)]
impl ClockConfig {
    pub const fn new() -> Self {
        Self {
            ext_ppqn: 24,
            clock_src: ClockSrc::Internal,
            reset_src: ResetSrc::None,
            internal_bpm: 120.0,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub struct QuantizerConfig {
    pub key: Key,
    pub tonic: Note,
}

#[allow(clippy::new_without_default)]
impl QuantizerConfig {
    pub const fn new() -> Self {
        Self {
            key: Key::Chromatic,
            tonic: Note::C,
        }
    }
}

#[derive(Copy, Clone, Serialize, PartialEq, Deserialize, PostcardBindings)]
#[repr(u16)]
pub enum ClockDivision {
    _1 = 1,
    _2 = 2,
    _4 = 4,
    _6 = 6,
    _8 = 8,
    _12 = 12,
    // 1 quarter note at 24 ppqn
    _24 = 24,
    // 1 bar at 24 ppqn
    _96 = 96,
    // 2 bars
    _192 = 192,
    // 4 bars
    _384 = 384,
}

#[derive(Clone, Serialize, PartialEq, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum AuxJackMode {
    None,
    ClockOut(ClockDivision),
    ResetOut,
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings)]
pub struct GlobalConfig {
    pub aux: [AuxJackMode; 3],
    pub clock: ClockConfig,
    pub i2c_mode: I2cMode,
    pub led_brightness: u8,
    pub midi: MidiConfig,
    pub quantizer: QuantizerConfig,
    pub takeover_mode: TakeoverMode,
}

#[allow(clippy::new_without_default)]
impl GlobalConfig {
    pub const fn new() -> Self {
        Self {
            aux: [
                AuxJackMode::ClockOut(ClockDivision::_1),
                AuxJackMode::None,
                AuxJackMode::None,
            ],
            clock: ClockConfig::new(),
            i2c_mode: I2cMode::Leader,
            led_brightness: 150,
            midi: MidiConfig::new(),
            quantizer: QuantizerConfig::new(),
            takeover_mode: TakeoverMode::Pickup,
        }
    }

    pub const fn validate(&mut self) {
        match self.clock.clock_src {
            ClockSrc::Atom => {
                self.aux[0] = AuxJackMode::None;
            }
            ClockSrc::Meteor => {
                self.aux[1] = AuxJackMode::None;
            }
            ClockSrc::Cube => {
                self.aux[2] = AuxJackMode::None;
            }
            _ => {}
        }
        match self.clock.reset_src {
            ResetSrc::Atom => {
                self.aux[0] = AuxJackMode::None;
            }
            ResetSrc::Meteor => {
                self.aux[1] = AuxJackMode::None;
            }
            ResetSrc::Cube => {
                self.aux[2] = AuxJackMode::None;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Curve {
    #[default]
    Linear,
    Logarithmic,
    Exponential,
}

impl Curve {
    pub fn at(&self, value: u16) -> u16 {
        let value = value.min(4095);
        match self {
            Curve::Linear => value,
            Curve::Exponential => CURVE_EXP[value as usize],
            Curve::Logarithmic => CURVE_LOG[value as usize],
        }
    }

    pub fn cycle(&self) -> Curve {
        match self {
            Curve::Linear => Curve::Exponential,
            Curve::Exponential => Curve::Logarithmic,
            Curve::Logarithmic => Curve::Linear,
        }
    }
}

impl FromValue for Curve {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Curve(c) => c,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Waveform {
    #[default]
    Triangle,
    Saw,
    SawInv,
    Square,
    Sine,
}

impl Waveform {
    pub fn at(&self, index: usize) -> u16 {
        let i = index % 4096;
        match self {
            Waveform::Sine => WAVEFORM_SINE[i],
            Waveform::Triangle => WAVEFORM_TRIANGLE[i],
            Waveform::Saw => WAVEFORM_SAW[i],
            Waveform::SawInv => WAVEFORM_SAW_INV[i],
            Waveform::Square => WAVEFORM_SQUARE[i],
        }
    }

    pub fn cycle(&self) -> Waveform {
        match self {
            Waveform::Sine => Waveform::Triangle,
            Waveform::Triangle => Waveform::Saw,
            Waveform::Saw => Waveform::SawInv,
            Waveform::SawInv => Waveform::Square,
            Waveform::Square => Waveform::Sine,
        }
    }
}

impl FromValue for Waveform {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Waveform(w) => w,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Color {
    #[default]
    White,
    Yellow,
    Orange,
    Red,
    Lime,
    Green,
    Cyan,
    SkyBlue,
    Blue,
    Violet,
    Pink,
    PaleGreen,
    Sand,
    Rose,
    Salmon,
    LightBlue,
    Custom(u8, u8, u8),
}

const PALETTE: [Color; 16] = [
    Color::White,
    Color::Pink,
    Color::Yellow,
    Color::Cyan,
    Color::Salmon,
    Color::Lime,
    Color::Orange,
    Color::Green,
    Color::SkyBlue,
    Color::Red,
    Color::PaleGreen,
    Color::Blue,
    Color::Sand,
    Color::Violet,
    Color::LightBlue,
    Color::Rose,
];

impl From<usize> for Color {
    fn from(value: usize) -> Self {
        PALETTE[value]
    }
}

impl From<Color> for RGB8 {
    fn from(value: Color) -> Self {
        match value {
            Color::White => WHITE,
            Color::Pink => PINK,
            Color::Yellow => YELLOW,
            Color::Cyan => CYAN,
            Color::Salmon => SALMON,
            Color::Lime => LIME,
            Color::Orange => ORANGE,
            Color::Green => GREEN,
            Color::SkyBlue => SKY_BLUE,
            Color::Red => RED,
            Color::PaleGreen => PALE_GREEN,
            Color::Blue => BLUE,
            Color::Sand => SAND,
            Color::Violet => VIOLET,
            Color::LightBlue => LIGHT_BLUE,
            Color::Rose => ROSE,
            Color::Custom(r, g, b) => RGB8 { r, g, b },
        }
    }
}

impl FromValue for Color {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Color(c) => c,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Brightness {
    Off,
    Low,
    Mid,
    High,
    Custom(u8),
}

impl From<Brightness> for u8 {
    fn from(value: Brightness) -> Self {
        match value {
            Brightness::Off => 0,
            Brightness::Low => 110,
            Brightness::Mid => 180,
            Brightness::High => 255,
            Brightness::Custom(value) => value,
        }
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PostcardBindings)]
pub enum AppIcon {
    #[default]
    Fader,
    AdEnv,
    Random,
    Euclid,
    Attenuate,
    Die,
    Quantize,
    Sequence,
    Note,
    EnvFollower,
    SoftRandom,
    Sine,
    NoteBox,
    SequenceSquare,
    NoteGrid,
    KnobRound,
    Stereo,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Serialize, PostcardBindings)]
pub enum Param {
    None,
    i32 {
        name: &'static str,
        min: i32,
        max: i32,
    },
    f32 {
        name: &'static str,
        min: f32,
        max: f32,
    },
    bool {
        name: &'static str,
    },
    Enum {
        name: &'static str,
        variants: &'static [&'static str],
    },
    Curve {
        name: &'static str,
        variants: &'static [Curve],
    },
    Waveform {
        name: &'static str,
        variants: &'static [Waveform],
    },
    Color {
        name: &'static str,
        variants: &'static [Color],
    },
    Range {
        name: &'static str,
        variants: &'static [Range],
    },
    Note {
        name: &'static str,
        variants: &'static [Note],
    },
    MidiCc {
        name: &'static str,
    },
    MidiChannel {
        name: &'static str,
    },
    MidiIn,
    MidiMode,
    MidiNote {
        name: &'static str,
    },
    MidiOut,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Value {
    i32(i32),
    f32(f32),
    bool(bool),
    Enum(usize),
    Curve(Curve),
    Waveform(Waveform),
    Color(Color),
    Range(Range),
    Note(Note),
    MidiCc(MidiCc),
    MidiChannel(MidiChannel),
    MidiIn(MidiIn),
    MidiMode(MidiMode),
    MidiNote(MidiNote),
    MidiOut(MidiOut),
}

impl From<Curve> for Value {
    fn from(value: Curve) -> Self {
        Value::Curve(value)
    }
}

impl From<Waveform> for Value {
    fn from(value: Waveform) -> Self {
        Value::Waveform(value)
    }
}

impl From<Color> for Value {
    fn from(value: Color) -> Self {
        Value::Color(value)
    }
}

impl From<Range> for Value {
    fn from(value: Range) -> Self {
        Value::Range(value)
    }
}

impl From<Note> for Value {
    fn from(value: Note) -> Self {
        Value::Note(value)
    }
}

impl From<MidiCc> for Value {
    fn from(value: MidiCc) -> Self {
        Value::MidiCc(value)
    }
}

impl From<MidiChannel> for Value {
    fn from(value: MidiChannel) -> Self {
        Value::MidiChannel(value)
    }
}

impl From<MidiIn> for Value {
    fn from(value: MidiIn) -> Self {
        Value::MidiIn(value)
    }
}

impl From<MidiMode> for Value {
    fn from(value: MidiMode) -> Self {
        Value::MidiMode(value)
    }
}

impl From<MidiNote> for Value {
    fn from(value: MidiNote) -> Self {
        Value::MidiNote(value)
    }
}

impl From<MidiOut> for Value {
    fn from(value: MidiOut) -> Self {
        Value::MidiOut(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::i32(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::f32(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::bool(value)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Value::Enum(value)
    }
}

#[derive(Deserialize, PostcardBindings)]
pub enum ConfigMsgIn {
    Ping,
    GetAllApps,
    GetGlobalConfig,
    SetGlobalConfig(GlobalConfig),
    GetLayout,
    SetLayout(Layout),
    GetAllAppParams,
    GetAppParams {
        layout_id: u8,
    },
    SetAppParams {
        layout_id: u8,
        values: [Option<Value>; APP_MAX_PARAMS],
    },
    FactoryReset,
}

#[derive(Clone, Serialize, PostcardBindings)]
#[allow(clippy::large_enum_variant)]
pub enum ConfigMsgOut<'a> {
    Pong,
    BatchMsgStart(usize),
    BatchMsgEnd,
    GlobalConfig(GlobalConfig),
    Layout(Layout),
    AppConfig(u8, usize, ConfigMeta<'a>),
    AppState(u8, &'a [Value]),
}

pub struct Config<const N: usize> {
    len: usize,
    name: &'static str,
    description: &'static str,
    params: [Param; N],
    color: Color,
    icon: AppIcon,
}

impl<const N: usize> Config<N> {
    pub const fn new(
        name: &'static str,
        description: &'static str,
        color: Color,
        icon: AppIcon,
    ) -> Self {
        assert!(N <= APP_MAX_PARAMS, "Too many params");
        Config {
            color,
            description,
            icon,
            len: 0,
            name,
            params: [const { Param::None }; N],
        }
    }

    pub const fn add_param(mut self, param: Param) -> Self {
        self.params[self.len] = param;
        let new_len = self.len + 1;
        Config {
            color: self.color,
            description: self.description,
            icon: self.icon,
            len: new_len,
            name: self.name,
            params: self.params,
        }
    }

    pub fn get_meta(&self) -> ConfigMeta<'_> {
        (
            N,
            self.name,
            self.description,
            self.color,
            self.icon,
            &self.params,
        )
    }
}

/// Supported DAC ranges
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PostcardBindings, PartialEq, Eq)]
#[repr(u8)]
pub enum Range {
    // 0 - 10V
    #[default]
    _0_10V,
    // 0 - 5V
    _0_5V,
    // -5 - 5V
    _Neg5_5V,
}

impl Range {
    // TODO: We might want to not need this in apps (handle it differently)
    pub fn is_bipolar(&self) -> bool {
        *self == Range::_Neg5_5V
    }
}

impl From<Range> for DACRANGE {
    fn from(value: Range) -> Self {
        match value {
            Range::_0_10V => DACRANGE::Rg0_10v,
            Range::_0_5V => DACRANGE::Rg0_10v,
            Range::_Neg5_5V => DACRANGE::RgNeg5_5v,
        }
    }
}
impl From<Range> for ADCRANGE {
    fn from(value: Range) -> Self {
        match value {
            Range::_0_10V => ADCRANGE::Rg0_10v,
            Range::_0_5V => ADCRANGE::Rg0_10v,
            Range::_Neg5_5V => ADCRANGE::RgNeg5_5v,
        }
    }
}

impl FromValue for Range {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Range(r) => r,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub struct MidiCc(u8);

impl FromValue for MidiCc {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiCc(m) => m,
            _ => Self::default(),
        }
    }
}

impl From<u8> for MidiCc {
    fn from(value: u8) -> Self {
        Self(value.min(127))
    }
}

impl From<MidiCc> for u7 {
    fn from(value: MidiCc) -> Self {
        u7::from_int_lossy(value.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub struct MidiChannel(u8);

impl FromValue for MidiChannel {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiChannel(m) => m,
            _ => Self::default(),
        }
    }
}

impl Default for MidiChannel {
    fn default() -> Self {
        MidiChannel(1)
    }
}

impl From<u8> for MidiChannel {
    fn from(value: u8) -> Self {
        Self(value.min(16))
    }
}

impl From<MidiChannel> for u4 {
    fn from(value: MidiChannel) -> Self {
        u4::from_int_lossy(value.0.saturating_sub(1).min(15))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, PostcardBindings)]
// [usb, din]
pub struct MidiIn(pub [bool; 2]);

impl Default for MidiIn {
    fn default() -> Self {
        Self([true; 2])
    }
}

impl FromValue for MidiIn {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiIn(m) => m,
            _ => Self::default(),
        }
    }
}

impl MidiIn {
    pub fn is_some(&self) -> bool {
        self.0.iter().any(|i| *i)
    }

    pub fn is_none(&self) -> bool {
        !self.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub struct MidiNote(u8);

impl FromValue for MidiNote {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiNote(m) => m,
            _ => Self::default(),
        }
    }
}

impl From<u8> for MidiNote {
    fn from(value: u8) -> Self {
        Self(value.min(127))
    }
}

impl From<i32> for MidiNote {
    fn from(value: i32) -> Self {
        Self(value.clamp(0, 127) as u8)
    }
}

impl From<MidiNote> for u7 {
    fn from(value: MidiNote) -> Self {
        u7::from_int_lossy(value.0)
    }
}

impl Add<MidiNote> for MidiNote {
    type Output = Self;

    fn add(self, rhs: MidiNote) -> Self::Output {
        Self(self.0.saturating_add(rhs.0).min(127))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
#[repr(u8)]
pub enum MidiMode {
    #[default]
    Note,
    Cc,
}

impl FromValue for MidiMode {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiMode(m) => m,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, PostcardBindings)]
// [usb, out1, out2]
pub struct MidiOut(pub [bool; 3]);

impl FromValue for MidiOut {
    fn from_value(value: Value) -> Self {
        match value {
            Value::MidiOut(m) => m,
            _ => Self::default(),
        }
    }
}

impl Default for MidiOut {
    fn default() -> Self {
        Self([true; 3])
    }
}

impl MidiOut {
    pub fn is_some(&self) -> bool {
        self.0.iter().any(|i| *i)
    }

    pub fn is_none(&self) -> bool {
        !self.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{Layout, GLOBAL_CHANNELS};
    use heapless::Vec;

    fn mock_get_channels(app_id: u8) -> Option<usize> {
        match app_id {
            1 => Some(1), // App 1 takes 2 channels
            2 => Some(4), // App 2 takes 4 channels
            3 => Some(3), // App 3 takes 3 channels
            _ => None,    // Any other app_id is invalid
        }
    }

    #[test]
    fn validate_no_changes() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        layout.0[0] = Some((1, 1, 0));
        layout.0[4] = Some((2, 4, 1));
        let original_layout = layout.0;

        let changed = layout.validate(mock_get_channels);

        assert!(!changed);
        assert_eq!(layout.0, original_layout);
    }

    #[test]
    fn validate_removes_overlapping() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        // App 2 is valid (takes 4 channels: 0, 1, 2, 3)
        layout.0[0] = Some((2, 4, 0));
        // App 3 overlaps with App 2 (tries to start at channel 2 but channels 2, 3 overlap with App 2)
        layout.0[2] = Some((3, 3, 1));
        // App 1 is valid and doesn't overlap
        layout.0[5] = Some((1, 1, 2));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // App 2 should remain (processed first)
        assert_eq!(layout.0[0], Some((2, 4, 0)));
        // App 3 should be removed (overlaps)
        assert_eq!(layout.0[2], None);
        // App 1 should remain
        assert_eq!(layout.0[5], Some((1, 1, 2)));
    }

    #[test]
    fn validate_removes_out_of_bounds() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        // This app goes from channel 14 up to 18, which is beyond GLOBAL_CHANNELS (16)
        layout.0[14] = Some((2, 4, 0));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // The out-of-bounds app should be removed
        assert_eq!(layout.0[14], None);
        assert!(layout.0.iter().all(|&app| app.is_none()));
    }

    #[test]
    fn validate_removes_invalid_id() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        // App ID 99 is not valid according to mock_get_channels
        layout.0[0] = Some((99, 2, 0));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        assert_eq!(layout.0[0], None);
    }

    #[test]
    fn validate_corrects_channel_size() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        // The stored channel size is 99, but mock_get_channels returns 1 for app_id 1
        layout.0[0] = Some((1, 99, 0));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // The channel size should be corrected to 1
        assert_eq!(layout.0[0], Some((1, 1, 0)));
    }

    #[test]
    fn validate_resolves_duplicate_and_oob_layout_ids() {
        let mut layout = Layout([None; GLOBAL_CHANNELS]);
        // Set up layout with duplicates and an out-of-bounds ID
        layout.0[0] = Some((1, 1, 5)); // Valid
        layout.0[2] = Some((1, 1, 2)); // Will be kept
        layout.0[4] = Some((1, 1, 2)); // Duplicate of ID 2
        layout.0[6] = Some((1, 1, 16)); // Out of bounds (>= GLOBAL_CHANNELS)
        layout.0[8] = Some((1, 1, 5)); // Duplicate of ID 5

        let changed = layout.validate(mock_get_channels);
        assert!(changed);

        // Expected layout_id assignments based on iteration order
        assert_eq!(layout.0[0].unwrap().2, 5);
        assert_eq!(layout.0[2].unwrap().2, 2);
        assert_eq!(layout.0[4].unwrap().2, 0); // First free ID
        assert_eq!(layout.0[6].unwrap().2, 1); // Second free ID
        assert_eq!(layout.0[8].unwrap().2, 3); // Third free ID

        // Verify all final layout_ids are unique
        let mut final_ids: Vec<u8, { GLOBAL_CHANNELS }> = Vec::new();
        for (_, _, _, layout_id) in layout.iter() {
            assert!(!final_ids.contains(&layout_id));
            final_ids.push(layout_id).unwrap();
        }
    }
}
