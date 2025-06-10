#![no_std]

use heapless::Vec;
use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

use libfp::constants::{
    GLOBAL_CHANNELS, WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SINE, WAVEFORM_TRIANGLE,
};

/// Maximum number of params per app
pub const APP_MAX_PARAMS: usize = 4;

pub type ConfigMeta<'a> = (usize, &'a str, &'a str, &'a [Param]);

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

#[derive(Clone)]
pub struct Layout {
    pub apps: Vec<(u8, usize, usize), GLOBAL_CHANNELS>,
    pub last: usize,
}

#[allow(clippy::new_without_default)]
impl Layout {
    pub const fn new() -> Self {
        Self {
            apps: Vec::new(),
            last: 0,
        }
    }

    pub fn push(&mut self, app: (u8, usize, usize)) {
        if !self.apps.is_full() {
            self.apps.push(app).expect("Vec should not be full");
        }
    }

    pub fn set_last(&mut self, last: usize) {
        self.last = last;
    }
}

#[derive(Clone)]
pub struct GlobalConfig {
    pub clock_src: ClockSrc,
    pub reset_src: ClockSrc,
    pub layout: Layout,
}

#[allow(clippy::new_without_default)]
impl GlobalConfig {
    pub const fn new() -> Self {
        Self {
            clock_src: ClockSrc::Internal,
            reset_src: ClockSrc::None,
            layout: Layout::new(),
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
            Waveform::Rect => WAVEFORM_RECT[i],
        }
    }

    pub fn cycle(&self) -> Waveform {
        match self {
            Waveform::Sine => Waveform::Triangle,
            Waveform::Triangle => Waveform::Saw,
            Waveform::Saw => Waveform::Rect,
            Waveform::Rect => Waveform::Sine,
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
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PostcardBindings)]
pub enum Value {
    None,
    i32(i32),
    f32(f32),
    bool(bool),
    Enum(usize),
    Curve(Curve),
    Waveform(Waveform),
}

impl From<Curve> for Value {
    fn from(value: Curve) -> Self {
        Value::Curve(value)
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

#[derive(Deserialize, PostcardBindings)]
pub enum ConfigMsgIn {
    Ping,
    GetAllApps,
    GetLayout,
    SetLayout([u8; 16]),
    SetAppParam {
        start_channel: usize,
        param_slot: usize,
        value: Value,
    },
}

#[derive(Clone, Serialize, PostcardBindings)]
pub enum ConfigMsgOut<'a> {
    Pong,
    BatchMsgStart(usize),
    BatchMsgEnd,
    GlobalConfig(ClockSrc, ClockSrc, &'a [(u8, usize, usize)]),
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
