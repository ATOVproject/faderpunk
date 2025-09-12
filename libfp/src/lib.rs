#![no_std]

use embassy_time::Duration;
use max11300::config::DACRANGE;
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

use constants::{
    CURVE_EXP, CURVE_LOG, WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SAW_INV, WAVEFORM_SINE,
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
pub const APP_MAX_PARAMS: usize = 8;

/// Length of the startup animation
pub const STARTUP_ANIMATION_DURATION: Duration = Duration::from_secs(2);

/// Rang in which the LED brightness is scaled
pub const LED_BRIGHTNESS_RANGE: core::ops::Range<u8> = 185..255;

pub const CALIBRATION_SCALE_FACTOR: i64 = 1 << 16;
pub const CALIBRATION_VERSION_LATEST: u8 = 2;
pub const CALIB_FILE_MAGIC: [u8; 4] = *b"FPBC";

pub type ConfigMeta<'a> = (usize, &'a str, &'a str, Color, AppIcon, &'a [Param]);

/// The config layout is a layout with all the apps in the appropriate spots
// (app_id, channels)
type InnerLayout = [Option<(u8, usize)>; GLOBAL_CHANNELS];

#[derive(Clone, Serialize, Deserialize, PostcardBindings)]
pub struct Layout(pub InnerLayout);

#[allow(clippy::new_without_default)]
impl Layout {
    pub const fn new() -> Self {
        Self([None; GLOBAL_CHANNELS])
    }

    pub fn validate(&mut self, get_channels: fn(u8) -> Option<usize>) -> bool {
        let mut validated: InnerLayout = [None; GLOBAL_CHANNELS];
        let mut occupied = [false; GLOBAL_CHANNELS];

        for (app_id, start_channel, _channels) in self.iter() {
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
                validated[start_channel] = Some((app_id, channels));
            }
        }

        let changed = self.0 != validated;
        self.0 = validated;

        changed
    }

    pub fn iter(&self) -> LayoutIter<'_> {
        self.into_iter()
    }
}

pub struct LayoutIter<'a> {
    slice: &'a [Option<(u8, usize)>],
    index: usize,
}

impl Iterator for LayoutIter<'_> {
    // (app_id, start_channel, channels)
    type Item = (u8, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        // Skip None values
        while self.index < self.slice.len() {
            if let Some(value) = self.slice[self.index] {
                let idx = self.index;
                self.index += 1;
                return Some((value.0, idx, value.1));
            }
            self.index += 1;
        }
        None
    }
}

impl<'a> IntoIterator for &'a Layout {
    type Item = (u8, usize, usize);
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
        match value {
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
            _ => unreachable!(),
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

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
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

#[derive(Clone, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub struct ClockConfig {
    pub clock_src: ClockSrc,
    pub ext_ppqn: u8,
    pub reset_src: ClockSrc,
    pub internal_bpm: f32,
}

#[allow(clippy::new_without_default)]
impl ClockConfig {
    pub const fn new() -> Self {
        Self {
            ext_ppqn: 24,
            clock_src: ClockSrc::Internal,
            reset_src: ClockSrc::None,
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
    pub quantizer: QuantizerConfig,
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
            quantizer: QuantizerConfig::new(),
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
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Curve {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
}

impl Curve {
    pub fn at(&self, value: u16) -> u16 {
        let value = value.clamp(0, 4095);
        match self {
            Curve::Linear => value,
            Curve::Logarithmic => CURVE_LOG[value as usize],
            Curve::Exponential => CURVE_EXP[value as usize],
        }
    }

    pub fn cycle(&self) -> Curve {
        match self {
            Curve::Linear => Curve::Logarithmic,
            Curve::Logarithmic => Curve::Exponential,
            Curve::Exponential => Curve::Linear,
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
    Rect,
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
            Waveform::Rect => WAVEFORM_RECT[i],
        }
    }

    pub fn cycle(&self) -> Waveform {
        match self {
            Waveform::Sine => Waveform::Triangle,
            Waveform::Triangle => Waveform::Saw,
            Waveform::Saw => Waveform::SawInv,
            Waveform::SawInv => Waveform::Rect,
            Waveform::Rect => Waveform::Sine,
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
    Lowest,
    Lower,
    Low,
    Default,
    Custom(u8),
}

impl From<Brightness> for u8 {
    fn from(value: Brightness) -> Self {
        match value {
            Brightness::Lowest => 80,
            Brightness::Lower => 125,
            Brightness::Low => 190,
            Brightness::Default => 255,
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
    Float {
        name: &'static str,
    },
    Bool {
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

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::i32(value)
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
    GetAppParams {
        start_channel: usize,
    },
    SetAppParams {
        start_channel: usize,
        values: [Option<Value>; APP_MAX_PARAMS],
    },
}

#[derive(Clone, Serialize, PostcardBindings)]
pub enum ConfigMsgOut<'a> {
    Pong,
    BatchMsgStart(usize),
    BatchMsgEnd,
    GlobalConfig(GlobalConfig),
    Layout(Layout),
    AppConfig(u8, usize, ConfigMeta<'a>),
    AppState(usize, &'a [Value]),
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

impl FromValue for Range {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Range(r) => r,
            _ => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Layout;

    fn mock_get_channels(app_id: u8) -> Option<usize> {
        match app_id {
            1 => Some(2), // App 1 takes 2 channels
            2 => Some(4), // App 2 takes 4 channels
            3 => Some(3), // App 3 takes 3 channels
            _ => None,    // Any other app_id is invalid
        }
    }

    #[test]
    fn validate_no_changes() {
        let mut layout = Layout::new();
        layout.0[0] = Some((1, 2));
        layout.0[4] = Some((2, 4));
        let original_layout = layout.0;

        let changed = layout.validate(mock_get_channels);

        assert!(!changed);
        assert_eq!(layout.0, original_layout);
    }

    #[test]
    fn validate_removes_overlapping() {
        let mut layout = Layout::new();
        // App 1 is valid
        layout.0[0] = Some((1, 2));
        // App 3 overlaps with App 1
        layout.0[1] = Some((3, 3));
        // App 2 is also valid and does not overlap with App 1
        layout.0[5] = Some((2, 4));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // App 1 should remain
        assert_eq!(layout.0[0], Some((1, 2)));
        // App 3 should be removed
        assert_eq!(layout.0[1], None);
        // App 2 should remain
        assert_eq!(layout.0[5], Some((2, 4)));
    }

    #[test]
    fn validate_removes_out_of_bounds() {
        let mut layout = Layout::new();
        // This app goes from channel 14 up to 18, which is beyond GLOBAL_CHANNELS (16)
        layout.0[14] = Some((2, 4));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // The out-of-bounds app should be removed
        assert_eq!(layout.0[14], None);
        assert!(layout.0.iter().all(|&app| app.is_none()));
    }

    #[test]
    fn validate_removes_invalid_id() {
        let mut layout = Layout::new();
        // App ID 99 is not valid according to mock_get_channels
        layout.0[0] = Some((99, 2));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        assert_eq!(layout.0[0], None);
    }

    #[test]
    fn validate_corrects_channel_size() {
        let mut layout = Layout::new();
        // The stored channel size is 99, but mock_get_channels returns 2 for app_id 1
        layout.0[0] = Some((1, 99));

        let changed = layout.validate(mock_get_channels);

        assert!(changed);
        // The channel size should be corrected to 2
        assert_eq!(layout.0[0], Some((1, 2)));
    }
}
