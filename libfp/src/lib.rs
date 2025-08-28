#![no_std]

use embassy_time::Duration;
use max11300::config::DACRANGE;
use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

pub mod constants;
pub mod ext;
pub mod i2c_proto;
pub mod quantizer;
pub mod types;
pub mod utils;

use constants::{
    ATOV_BLUE, ATOV_PURPLE, ATOV_RED, ATOV_WHITE, ATOV_YELLOW, CURVE_EXP, CURVE_LOG, WAVEFORM_RECT,
    WAVEFORM_SAW, WAVEFORM_SAW_INV, WAVEFORM_SINE, WAVEFORM_TRIANGLE,
};
use smart_leds::RGB8;

/// Total channel size of this device
pub const GLOBAL_CHANNELS: usize = 16;

/// The devices I2C address (as a follower)
pub const I2C_ADDRESS: u16 = 0x56;
pub const I2C_ADDRESS_CALIBRATION: u16 = 0x57;

/// Maximum number of params per app
pub const APP_MAX_PARAMS: usize = 8;

/// Length of the startup animation
pub const STARTUP_ANIMATION_DURATION: Duration = Duration::from_secs(2);

pub type ConfigMeta<'a> = (usize, &'a str, &'a str, &'a [Param]);

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

pub trait FromValue: Sized + Default + Copy {
    fn from_value(value: Value) -> Self;
}

impl FromValue for bool {
    fn from_value(value: Value) -> Self {
        match value {
            Value::bool(i) => i,
            _ => Self::default(),
        }
    }
}

impl FromValue for i32 {
    fn from_value(value: Value) -> Self {
        match value {
            Value::i32(i) => i,
            _ => Self::default(),
        }
    }
}

impl FromValue for usize {
    fn from_value(value: Value) -> Self {
        match value {
            Value::Enum(i) => i,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, PostcardBindings)]
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
pub enum I2cMode {
    Calibration,
    Leader,
    Follower,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
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

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub enum Key {
    Chromatic = 0b111111111111,
    Major = 0b101011010101,
    Minor = 0b101101011010,
    PentatonicMajor = 0b101010010100,
    PentatonicMinor = 0b100101010010,
    Purvi = 0b110010111001,
    Todi = 0b110100111001,
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings)]
pub struct GlobalConfig {
    pub clock_src: ClockSrc,
    pub reset_src: ClockSrc,
    pub i2c_mode: I2cMode,
    pub quantizer_key: Key,
    pub quantizer_tonic: Note,
}

#[allow(clippy::new_without_default)]
impl GlobalConfig {
    pub const fn new() -> Self {
        Self {
            clock_src: ClockSrc::Internal,
            reset_src: ClockSrc::None,
            i2c_mode: I2cMode::Follower,
            quantizer_key: Key::PentatonicMajor,
            quantizer_tonic: Note::C,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PostcardBindings)]
pub enum Curve {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
}

impl Curve {
    pub fn at(&self, index: usize) -> u16 {
        let index = index.clamp(0, 4095);
        match self {
            Curve::Linear => index as u16,
            Curve::Logarithmic => CURVE_LOG[index],
            Curve::Exponential => CURVE_EXP[index],
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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PostcardBindings)]
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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PostcardBindings)]
pub enum Color {
    #[default]
    White,
    Red,
    Blue,
    Yellow,
    Purple,
}

impl From<Color> for RGB8 {
    fn from(value: Color) -> Self {
        match value {
            Color::White => ATOV_WHITE,
            Color::Red => ATOV_RED,
            Color::Blue => ATOV_BLUE,
            Color::Yellow => ATOV_YELLOW,
            Color::Purple => ATOV_PURPLE,
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
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PostcardBindings)]
pub enum Value {
    i32(i32),
    f32(f32),
    bool(bool),
    Enum(usize),
    Curve(Curve),
    Waveform(Waveform),
    Color(Color),
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
}

impl<const N: usize> Config<N> {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        assert!(N <= APP_MAX_PARAMS, "Too many params");
        Config {
            description,
            len: 0,
            name,
            params: [const { Param::None }; N],
        }
    }

    pub const fn add_param(mut self, param: Param) -> Self {
        self.params[self.len] = param;
        let new_len = self.len + 1;
        Config {
            description: self.description,
            len: new_len,
            name: self.name,
            params: self.params,
        }
    }

    pub fn get_meta(&self) -> ConfigMeta<'_> {
        (N, self.name, self.description, &self.params)
    }
}

/// Supported DAC ranges
#[repr(u8)]
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub enum Range {
    // 0 - 10V
    _0_10V,
    // 0 - 5V
    _0_5V,
    // -5 - 5V
    _Neg5_5V,
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
