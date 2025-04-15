#![no_std]

use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

use libfp::constants::{WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SINE, WAVEFORM_TRIANGLE};

/// Maximum number of params per app
pub const MAX_PARAMS: usize = 16;

pub trait FromValue: Sized + Default + Copy {
    fn from_value(value: Value) -> Self;
}

impl FromValue for i32 {
    fn from_value(value: Value) -> Self {
        match value {
            Value::i32(i) => i,
            _ => Self::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ClockSrc {
    None,
    Atom,
    Meteor,
    Cube,
    Internal,
    MidiIn,
    MidiUsb,
}

#[derive(Clone, Copy)]
pub struct GlobalConfig<'a> {
    pub clock_src: ClockSrc,
    pub reset_src: ClockSrc,
    pub layout: &'a [usize],
}

impl Default for GlobalConfig<'_> {
    fn default() -> Self {
        Self {
            clock_src: ClockSrc::Internal,
            reset_src: ClockSrc::None,
            layout: &[1; 16],
        }
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PostcardBindings)]
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

#[derive(Clone, Copy, Serialize, Deserialize, PostcardBindings)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Rect,
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
        min: usize,
        max: usize,
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
#[derive(Clone, Copy, Deserialize, PostcardBindings)]
pub enum Value {
    None,
    i32(i32),
    Float(f32),
    Bool(bool),
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

#[derive(Clone, Copy, Deserialize, PostcardBindings)]
pub enum ConfigMsgIn {
    Ping,
    GetApps,
}

#[derive(Clone, Copy, Serialize, PostcardBindings)]
pub enum ConfigMsgOut<'a> {
    Pong,
    BatchMsgStart(usize),
    BatchMsgEnd,
    AppConfig((&'a str, &'a str, &'a [Param])),
}

pub struct Config<const N: usize> {
    len: usize,
    name: &'static str,
    description: &'static str,
    params: [Param; N],
}

impl<const N: usize> Config<N> {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        assert!(N <= MAX_PARAMS, "Too many params");
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

    pub fn get_meta(&self) -> (&str, &str, &[Param]) {
        (self.name, self.description, &self.params)
    }
}
