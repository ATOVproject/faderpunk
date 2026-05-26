#![no_std]

use core::ops::Add;

use embassy_time::Duration;
use heapless::Vec;
use max11300::config::{ADCRANGE, DACRANGE};
use midly::num::{u4, u7};
use minicbor::{Decode, Encode};
use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

pub mod colors;
pub mod constants;
pub mod ext;
pub mod fp_grids_lib;
pub mod i2c_proto;
pub mod latch;
pub mod quantizer;
pub mod types;
pub mod utils;

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

#[derive(Clone, Serialize, Deserialize, PostcardBindings, Encode, Decode)]
#[cbor(transparent)]
pub struct Layout(#[n(0)] pub InnerLayout);

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

/// Persisted in `GlobalConfig` via CBOR. New variants may be appended with the
/// next free `#[n(N)]` tag without a migration. **Removing** a variant
/// requires a one-shot FRAM migration (see `storage::migrate_fram`) — old
/// stored data containing the removed tag would otherwise fail to decode.
#[derive(
    Clone, Copy, Default, PartialEq, Serialize, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(index_only)]
#[repr(u8)]
pub enum ClockSrc {
    #[n(0)]
    None,
    #[n(1)]
    Atom,
    #[n(2)]
    Meteor,
    #[n(3)]
    Cube,
    #[default]
    #[n(4)]
    Internal,
    #[n(5)]
    MidiIn,
    #[n(6)]
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

/// Persisted in `GlobalConfig` via CBOR. New variants may be appended with the
/// next free `#[n(N)]` tag without a migration. **Removing** a variant
/// requires a one-shot FRAM migration (see `storage::migrate_fram`).
#[derive(
    Clone, Copy, Default, PartialEq, Serialize, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(index_only)]
#[repr(u8)]
pub enum ResetSrc {
    #[default]
    #[n(0)]
    None,
    #[n(1)]
    Atom,
    #[n(2)]
    Meteor,
    #[n(3)]
    Cube,
}

/// Persisted in `GlobalConfig` via CBOR. New variants may be appended with the
/// next free `#[n(N)]` tag without a migration. **Removing** a variant
/// requires a one-shot FRAM migration (see `storage::migrate_fram`).
#[derive(Clone, Default, Serialize, Deserialize, PostcardBindings, Encode, Decode)]
#[cbor(index_only)]
#[repr(u8)]
pub enum I2cMode {
    #[n(0)]
    Calibration,
    #[default]
    #[n(1)]
    Leader,
    #[n(2)]
    Follower,
}

/// Persisted in `GlobalConfig` via CBOR (inside `QuantizerConfig`). New
/// variants may be appended with the next free `#[n(N)]` tag without a
/// migration. **Removing** a variant requires a one-shot FRAM migration (see
/// `storage::migrate_fram`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(index_only)]
#[repr(u8)]
pub enum Note {
    #[default]
    #[n(0)]
    C = 0,
    #[n(1)]
    CSharp = 1,
    #[n(2)]
    D = 2,
    #[n(3)]
    DSharp = 3,
    #[n(4)]
    E = 4,
    #[n(5)]
    F = 5,
    #[n(6)]
    FSharp = 6,
    #[n(7)]
    G = 7,
    #[n(8)]
    GSharp = 8,
    #[n(9)]
    A = 9,
    #[n(10)]
    ASharp = 10,
    #[n(11)]
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

/// Persisted in `GlobalConfig` via CBOR (inside `QuantizerConfig`). New
/// variants may be appended with the next free `#[n(N)]` tag without a
/// migration. **Removing** a variant requires a one-shot FRAM migration (see
/// `storage::migrate_fram`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(index_only)]
#[repr(u8)]
pub enum Key {
    #[default]
    #[n(0)]
    Chromatic,
    #[n(1)]
    Ionian,
    #[n(2)]
    Dorian,
    #[n(3)]
    Phrygian,
    #[n(4)]
    Lydian,
    #[n(5)]
    Mixolydian,
    #[n(6)]
    Aeolian,
    #[n(7)]
    Locrian,
    #[n(8)]
    BluesMaj,
    #[n(9)]
    BluesMin,
    #[n(10)]
    PentatonicMaj,
    #[n(11)]
    PentatonicMin,
    #[n(12)]
    Folk,
    #[n(13)]
    Japanese,
    #[n(14)]
    Gamelan,
    #[n(15)]
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

/// Persisted in `GlobalConfig` via CBOR (inside `MidiOutConfig`). New variants
/// may be appended with the next free `#[n(N)]` tag without a migration.
/// **Removing** a variant requires a one-shot FRAM migration (see
/// `storage::migrate_fram`).
#[derive(
    Clone, Copy, Default, Serialize, Deserialize, PostcardBindings, PartialEq, Encode, Decode,
)]
pub enum MidiOutMode {
    #[n(0)]
    None,
    #[default]
    #[n(1)]
    Local,
    #[n(2)]
    MidiThru {
        #[n(0)]
        sources: MidiIn,
    },
    #[n(3)]
    MidiMerge {
        #[n(0)]
        sources: MidiIn,
    },
}

#[derive(Clone, Copy, Serialize, Deserialize, PostcardBindings, PartialEq, Encode, Decode)]
pub struct MidiOutConfig {
    #[n(0)]
    #[cbor(default)]
    pub send_clock: bool,
    #[n(1)]
    #[cbor(default)]
    pub send_transport: bool,
    #[n(2)]
    #[cbor(default)]
    pub mode: MidiOutMode,
}

impl Default for MidiOutConfig {
    fn default() -> Self {
        Self::new()
    }
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

#[derive(Clone, Serialize, Deserialize, PostcardBindings, PartialEq, Encode, Decode)]
pub struct MidiConfig {
    // [usb, out1, out2]
    #[n(0)]
    #[cbor(default)]
    pub outs: [MidiOutConfig; 3],
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::new_without_default)]
impl MidiConfig {
    pub const fn new() -> Self {
        Self {
            outs: [MidiOutConfig::new(); 3],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PostcardBindings, PartialEq, Encode, Decode)]
pub struct ClockConfig {
    #[n(0)]
    #[cbor(default)]
    pub clock_src: ClockSrc,
    #[n(1)]
    #[cbor(default)]
    pub ext_ppqn: u8,
    #[n(2)]
    #[cbor(default)]
    pub reset_src: ResetSrc,
    #[n(3)]
    #[cbor(default)]
    pub internal_bpm: f32,
    /// Deluge-style swing amount in `[-35, 35]`. `0` = straight.
    #[n(4)]
    #[cbor(default)]
    pub swing_amount: i8,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::new_without_default)]
impl ClockConfig {
    pub const fn new() -> Self {
        Self {
            ext_ppqn: 24,
            clock_src: ClockSrc::Internal,
            reset_src: ResetSrc::None,
            internal_bpm: 120.0,
            swing_amount: 0,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, PostcardBindings, PartialEq, Encode, Decode)]
pub struct QuantizerConfig {
    #[n(0)]
    #[cbor(default)]
    pub key: Key,
    #[n(1)]
    #[cbor(default)]
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

/// Persisted in `GlobalConfig` via CBOR (inside `AuxJackMode::ClockOut`). New
/// variants may be appended with the next free `#[n(N)]` tag without a
/// migration. **Removing** a variant requires a one-shot FRAM migration (see
/// `storage::migrate_fram`).
#[derive(
    Copy, Clone, Default, Serialize, PartialEq, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(index_only)]
#[repr(u16)]
pub enum ClockDivision {
    #[default]
    #[n(1)]
    _1 = 1,
    #[n(2)]
    _2 = 2,
    #[n(4)]
    _4 = 4,
    #[n(6)]
    _6 = 6,
    #[n(8)]
    _8 = 8,
    #[n(12)]
    _12 = 12,
    // 1 quarter note at 24 ppqn
    #[n(24)]
    _24 = 24,
    // 1 bar at 24 ppqn
    #[n(96)]
    _96 = 96,
    // 2 bars
    #[n(192)]
    _192 = 192,
    // 4 bars
    #[n(384)]
    _384 = 384,
}

/// Persisted in `GlobalConfig` via CBOR (inside `aux: [AuxJackMode; 3]`). New
/// variants may be appended with the next free `#[n(N)]` tag without a
/// migration. **Removing** a variant requires a one-shot FRAM migration (see
/// `storage::migrate_fram`).
#[derive(Clone, Default, Serialize, PartialEq, Deserialize, PostcardBindings, Encode, Decode)]
#[repr(u8)]
pub enum AuxJackMode {
    #[default]
    #[n(0)]
    None,
    #[n(1)]
    ClockOut(#[n(0)] ClockDivision),
    #[n(2)]
    ResetOut,
}

/// `GlobalConfig` is persisted to FRAM as CBOR. To keep the on-FRAM format
/// forward/backward compatible without writing a migration:
///
/// - **Every field has `#[cbor(default)]`.** Missing tags decode as
///   `Default::default()` instead of erroring. Removing a field is just
///   deleting the field; old stored data with that tag is silently skipped.
///   Adding a field is just declaring it with the next free tag — old data
///   without the tag falls back to its `Default`.
/// - **Tags are append-only.** Never reuse an `#[n(N)]` for a different
///   purpose; pick the next unused integer. Reusing a tag would silently
///   reinterpret old stored data.
/// - **Field types must implement `Default`** with a value that's safe if it
///   ever shows up on a device that's missing the field in FRAM.
///
/// This convention applies recursively to every type reachable from
/// `GlobalConfig` through fields tagged `#[cbor(default)]`. Things that *do*
/// require a one-shot migration (handled in `storage::migrate_fram`):
///
/// - Changing the type of an existing field (e.g. `u8 → u16`).
/// - Resizing fixed-size arrays (`[T; N]`) or tuples.
/// - Removing an enum variant while old data may still contain it.
#[derive(Clone, Serialize, Deserialize, PostcardBindings, Encode, Decode)]
pub struct GlobalConfig {
    #[n(0)]
    #[cbor(default)]
    pub aux: [AuxJackMode; 3],
    #[n(1)]
    #[cbor(default)]
    pub clock: ClockConfig,
    #[n(2)]
    #[cbor(default)]
    pub i2c_mode: I2cMode,
    #[n(3)]
    #[cbor(default)]
    pub led_brightness: u8,
    #[n(4)]
    #[cbor(default)]
    pub midi: MidiConfig,
    #[n(5)]
    #[cbor(default)]
    pub quantizer: QuantizerConfig,
    #[n(6)]
    #[cbor(default)]
    pub takeover_mode: TakeoverMode,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self::new()
    }
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
    MidiNrpn,
    VoltPerOct,
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
    MidiNrpn(bool),
    VoltPerOct(VoltPerOct),
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

/// Pitch CV standard: 1V/Oct (Eurorack) or 1.2V/Oct (Buchla)
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PostcardBindings, PartialEq)]
pub enum VoltPerOct {
    #[default]
    Standard,
    Buchla,
}

impl From<VoltPerOct> for Value {
    fn from(value: VoltPerOct) -> Self {
        Value::VoltPerOct(value)
    }
}

impl FromValue for VoltPerOct {
    fn from_value(value: Value) -> Self {
        match value {
            Value::VoltPerOct(v) => v,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, PostcardBindings)]
pub struct MidiCc(u16);

impl MidiCc {
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

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
        Self(value as u16)
    }
}

impl From<u16> for MidiCc {
    fn from(value: u16) -> Self {
        Self(value.min(16383))
    }
}

impl From<i32> for MidiCc {
    fn from(value: i32) -> Self {
        Self((value.clamp(0, 16383)) as u16)
    }
}

impl From<MidiCc> for u7 {
    fn from(value: MidiCc) -> Self {
        u7::from_int_lossy(value.0 as u8)
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

#[derive(
    Clone, Copy, Debug, PartialEq, Serialize, Deserialize, PostcardBindings, Encode, Decode,
)]
#[cbor(transparent)]
// [usb, din]
pub struct MidiIn(#[n(0)] pub [bool; 2]);

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

impl MidiNote {
    /// Transpose a MidiNote by +/- semitones
    pub fn transpose(&mut self, semitones: i8) -> Self {
        Self((self.0 as i8 + semitones).clamp(0, 127) as u8)
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

    // ---- CBOR storage / migration tests ----
    //
    // These pin down the behaviour the rest of the storage layer relies on:
    //   1. CBOR round-trips current persisted types.
    //   2. Adding a `#[cbor(default)]` field is forward-compatible: data written
    //      by an older shape decodes into the newer shape with the new field
    //      defaulted.
    //   3. Removing a field is backward-compatible: data written by an older
    //      shape decodes into the newer (slimmer) shape with the dropped field
    //      silently skipped.
    //   4. v1.8.2 postcard `GlobalConfig` data fails to decode as the current
    //      postcard `GlobalConfig` (which is what makes the migration's
    //      "current first, V0 fallback" strategy safe).

    use super::*;
    use minicbor::{Decode, Encode};
    use serde::{Deserialize, Serialize};

    fn cbor_encode_to_vec<T: Encode<()>>(value: &T) -> heapless::Vec<u8, 256> {
        let mut buf = [0u8; 256];
        let initial = buf.len();
        let mut writer: &mut [u8] = &mut buf[..];
        minicbor::encode(value, &mut writer).unwrap();
        let written = initial - writer.len();
        heapless::Vec::from_slice(&buf[..written]).unwrap()
    }

    #[test]
    fn cbor_round_trip_default_global_config() {
        let original = GlobalConfig::new();
        let encoded = cbor_encode_to_vec(&original);
        let decoded: GlobalConfig = minicbor::decode(&encoded).unwrap();

        assert_eq!(decoded.led_brightness, original.led_brightness);
        assert_eq!(
            decoded.clock.internal_bpm.to_bits(),
            original.clock.internal_bpm.to_bits()
        );
        assert_eq!(decoded.clock.swing_amount, original.clock.swing_amount);
        assert_eq!(decoded.i2c_mode as u8, I2cMode::Leader as u8);
    }

    #[test]
    fn cbor_field_added_decodes_with_default() {
        // V1: a struct shaped like an "older" version.
        #[derive(Encode, Decode)]
        struct V1 {
            #[n(0)]
            x: u32,
            #[n(1)]
            y: u32,
        }

        // V2: same tags, plus a new field that uses cbor(default).
        #[derive(Encode, Decode, Debug, PartialEq)]
        struct V2 {
            #[n(0)]
            #[cbor(default)]
            x: u32,
            #[n(1)]
            #[cbor(default)]
            y: u32,
            #[n(2)]
            #[cbor(default)]
            z: u32,
        }

        let v1 = V1 { x: 42, y: 7 };
        let bytes = cbor_encode_to_vec(&v1);
        let v2: V2 = minicbor::decode(&bytes).unwrap();
        assert_eq!(v2, V2 { x: 42, y: 7, z: 0 });
    }

    #[test]
    fn cbor_field_removed_skips_unknown_tag() {
        // V2: an "older" version that wrote a y field.
        #[derive(Encode, Decode)]
        struct V2 {
            #[n(0)]
            x: u32,
            #[n(1)]
            y: u32,
        }

        // V3: the y field has been removed.
        #[derive(Encode, Decode, Debug, PartialEq)]
        struct V3 {
            #[n(0)]
            x: u32,
        }

        let v2 = V2 { x: 42, y: 999 };
        let bytes = cbor_encode_to_vec(&v2);
        let v3: V3 = minicbor::decode(&bytes).unwrap();
        assert_eq!(v3, V3 { x: 42 });
    }

    #[test]
    fn cbor_postcard_data_does_not_decode_as_cbor() {
        // After migration, FRAM holds CBOR. If the schema header somehow gets
        // lost and migrate_fram runs on already-migrated data, the postcard
        // legacy decoders must reject the CBOR bytes (so we don't clobber).
        let original = GlobalConfig::new();
        let cbor_bytes = cbor_encode_to_vec(&original);

        let postcard_result: Result<GlobalConfig, _> = postcard::from_bytes(&cbor_bytes);
        assert!(
            postcard_result.is_err(),
            "postcard must reject CBOR-encoded GlobalConfig bytes"
        );
    }

    /// Mirror of v1.8.2's `ClockConfig` shape, used only by the migration
    /// regression tests below.
    #[derive(Serialize, Deserialize)]
    struct ClockConfigPreSwing {
        clock_src: ClockSrc,
        ext_ppqn: u8,
        reset_src: ResetSrc,
        internal_bpm: f32,
    }

    /// Mirror of v1.8.2's `GlobalConfig` shape (no `swing_amount` in clock).
    #[derive(Serialize, Deserialize)]
    struct GlobalConfigPreSwing {
        aux: [AuxJackMode; 3],
        clock: ClockConfigPreSwing,
        i2c_mode: I2cMode,
        led_brightness: u8,
        midi: MidiConfig,
        quantizer: QuantizerConfig,
        takeover_mode: TakeoverMode,
    }

    fn make_v18_default() -> GlobalConfigPreSwing {
        GlobalConfigPreSwing {
            aux: [
                AuxJackMode::ClockOut(ClockDivision::_1),
                AuxJackMode::None,
                AuxJackMode::None,
            ],
            clock: ClockConfigPreSwing {
                clock_src: ClockSrc::Internal,
                ext_ppqn: 24,
                reset_src: ResetSrc::None,
                internal_bpm: 120.0,
            },
            i2c_mode: I2cMode::Leader,
            led_brightness: 150,
            midi: MidiConfig::new(),
            quantizer: QuantizerConfig::new(),
            takeover_mode: TakeoverMode::Pickup,
        }
    }

    #[test]
    fn v18_postcard_data_fails_current_postcard_decode() {
        // The migration's correctness rests on this invariant: v1.8.2 stored
        // bytes are always 1 byte short of what the current GlobalConfig
        // postcard decoder needs (the missing `swing_amount`). Without it,
        // the "try current first" path could mis-decode v1.8.2 data as
        // pre-fix v1.9 data with shifted fields.
        let v18 = make_v18_default();
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v18, &mut buf).unwrap();

        let result: Result<GlobalConfig, _> = postcard::from_bytes(bytes);
        assert!(
            result.is_err(),
            "v1.8.2 postcard data must NOT decode as current postcard GlobalConfig"
        );
    }

    #[test]
    fn v18_postcard_data_decodes_as_pre_swing_shape() {
        // The fallback path: same legacy bytes, decoded with the v1.8.2-shaped
        // type, must succeed.
        let v18 = make_v18_default();
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v18, &mut buf).unwrap();

        let decoded: GlobalConfigPreSwing = postcard::from_bytes(bytes).unwrap();
        assert_eq!(decoded.led_brightness, 150);
        assert_eq!(decoded.clock.ext_ppqn, 24);
        assert_eq!(decoded.clock.internal_bpm.to_bits(), 120.0_f32.to_bits());
    }

    #[test]
    fn pre_fix_v19_postcard_data_decodes_as_current() {
        // Pre-fix v1.9 betas wrote postcard `GlobalConfig` with the swing
        // field present. The "try current first" path must accept it as-is.
        let original = GlobalConfig::new();
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&original, &mut buf).unwrap();

        let decoded: GlobalConfig = postcard::from_bytes(bytes).unwrap();
        assert_eq!(decoded.led_brightness, 150);
        assert_eq!(decoded.clock.swing_amount, 0);
        assert_eq!(decoded.i2c_mode as u8, I2cMode::Leader as u8);
    }

    #[test]
    fn brightness_round_trips_through_v18_migration() {
        // Reproduction of a real hardware report: brightness 111 on v1.8.2
        // came back as 100 after upgrading to fixed v1.9. Pin down that the
        // postcard-V0 → CBOR migration preserves an arbitrary u8 brightness
        // exactly (no clamping, no off-by-one).
        let mut v18 = make_v18_default();
        v18.led_brightness = 111;
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v18, &mut buf).unwrap();

        // Step 1: migration reads the legacy bytes via GlobalConfigPreSwing.
        let decoded_v0: GlobalConfigPreSwing = postcard::from_bytes(bytes).unwrap();
        assert_eq!(decoded_v0.led_brightness, 111);

        // Step 2: migration would convert to current GlobalConfig (we mirror
        // the From<GlobalConfigV0> conversion in storage.rs here).
        let migrated = GlobalConfig {
            aux: decoded_v0.aux,
            clock: ClockConfig {
                clock_src: decoded_v0.clock.clock_src,
                ext_ppqn: decoded_v0.clock.ext_ppqn,
                reset_src: decoded_v0.clock.reset_src,
                internal_bpm: decoded_v0.clock.internal_bpm,
                swing_amount: 0,
            },
            i2c_mode: decoded_v0.i2c_mode,
            led_brightness: decoded_v0.led_brightness,
            midi: decoded_v0.midi,
            quantizer: decoded_v0.quantizer,
            takeover_mode: decoded_v0.takeover_mode,
        };
        assert_eq!(migrated.led_brightness, 111);

        // Step 3: round-trip through CBOR (what migration actually writes,
        // and what every subsequent boot reads).
        let encoded = cbor_encode_to_vec(&migrated);
        let decoded: GlobalConfig = minicbor::decode(&encoded).unwrap();
        assert_eq!(decoded.led_brightness, 111);
    }

    /// Mirror of v1.7.0's `GlobalConfig` shape (no `takeover_mode`, no
    /// `swing_amount`).
    #[derive(Serialize, Deserialize)]
    struct GlobalConfigV170 {
        aux: [AuxJackMode; 3],
        clock: ClockConfigPreSwing,
        i2c_mode: I2cMode,
        led_brightness: u8,
        midi: MidiConfig,
        quantizer: QuantizerConfig,
    }

    fn make_v17_default() -> GlobalConfigV170 {
        GlobalConfigV170 {
            aux: [
                AuxJackMode::ClockOut(ClockDivision::_1),
                AuxJackMode::None,
                AuxJackMode::None,
            ],
            clock: ClockConfigPreSwing {
                clock_src: ClockSrc::Internal,
                ext_ppqn: 24,
                reset_src: ResetSrc::None,
                internal_bpm: 120.0,
            },
            i2c_mode: I2cMode::Leader,
            led_brightness: 150,
            midi: MidiConfig::new(),
            quantizer: QuantizerConfig::new(),
        }
    }

    #[test]
    fn v17_postcard_data_fails_current_and_v18_postcard_decode() {
        // v1.7.0 data is 2 bytes shorter than current and 1 byte shorter than
        // v1.8.x. Both larger shapes must fail so the V170 fallback is the one
        // that actually runs.
        let v17 = make_v17_default();
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v17, &mut buf).unwrap();

        let current: Result<GlobalConfig, _> = postcard::from_bytes(bytes);
        assert!(current.is_err(), "v1.7.0 must NOT decode as current");
        let v18: Result<GlobalConfigPreSwing, _> = postcard::from_bytes(bytes);
        assert!(v18.is_err(), "v1.7.0 must NOT decode as v1.8.x");
    }

    #[test]
    fn v17_postcard_data_decodes_as_v170_shape() {
        let v17 = make_v17_default();
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v17, &mut buf).unwrap();

        let decoded: GlobalConfigV170 = postcard::from_bytes(bytes).unwrap();
        assert_eq!(decoded.led_brightness, 150);
        assert_eq!(decoded.clock.internal_bpm.to_bits(), 120.0_f32.to_bits());
    }

    #[test]
    fn brightness_round_trips_through_v170_migration() {
        // Same hardware-bug regression as the v1.8.x test, one version older:
        // brightness 111 set on v1.7.0 must survive the postcard-V170 → CBOR
        // migration unchanged. `swing_amount` and `takeover_mode` come back as
        // their `Default::default()` values.
        let mut v17 = make_v17_default();
        v17.led_brightness = 111;
        let mut buf = [0u8; 256];
        let bytes = postcard::to_slice(&v17, &mut buf).unwrap();

        let decoded_v17: GlobalConfigV170 = postcard::from_bytes(bytes).unwrap();
        assert_eq!(decoded_v17.led_brightness, 111);

        // Mirror the From<GlobalConfigV170> conversion from storage.rs.
        let migrated = GlobalConfig {
            aux: decoded_v17.aux,
            clock: ClockConfig {
                clock_src: decoded_v17.clock.clock_src,
                ext_ppqn: decoded_v17.clock.ext_ppqn,
                reset_src: decoded_v17.clock.reset_src,
                internal_bpm: decoded_v17.clock.internal_bpm,
                swing_amount: 0,
            },
            i2c_mode: decoded_v17.i2c_mode,
            led_brightness: decoded_v17.led_brightness,
            midi: decoded_v17.midi,
            quantizer: decoded_v17.quantizer,
            takeover_mode: TakeoverMode::Pickup,
        };
        assert_eq!(migrated.led_brightness, 111);
        assert_eq!(migrated.clock.swing_amount, 0);

        let encoded = cbor_encode_to_vec(&migrated);
        let decoded: GlobalConfig = minicbor::decode(&encoded).unwrap();
        assert_eq!(decoded.led_brightness, 111);
        assert_eq!(decoded.clock.swing_amount, 0);
    }
}
