#![no_std]

use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

use libfp::constants::{WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SINE, WAVEFORM_TRIANGLE};

/// Maximum number of params per app
pub const MAX_APP_PARAMS: usize = 16;

#[derive(Clone, Copy, Default, Serialize, Deserialize, PostcardBindings)]
pub enum Curve {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
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

#[derive(Clone, Copy, Serialize, PostcardBindings)]
pub enum Param {
    None,
    Int {
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

#[derive(Clone, Copy, Deserialize, PostcardBindings)]
pub enum Value {
    None,
    Int(i32),
    Float(f32),
    Bool(bool),
    Enum(usize),
    Curve(Curve),
    Waveform(Waveform),
}

#[derive(Clone, Copy, Deserialize, PostcardBindings)]
pub enum ConfigMsgIn {
    Ping,
    GetApps,
}

#[derive(Clone, Serialize, PostcardBindings)]
pub enum ConfigMsgOut<'a> {
    Pong,
    BatchMsgStart(usize),
    BatchMsgEnd,
    AppConfig((&'a str, &'a str, Option<&'a [Param]>, Option<&'a [u8]>)),
}

pub struct Config<const N: usize> {
    len: usize,
    name: &'static str,
    description: &'static str,
    params: [Param; N],
}

impl<const N: usize> Config<N> {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        assert!(N <= MAX_APP_PARAMS, "Too many params");
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

    pub fn get(&self) -> (&str, &str, &[Param]) {
        (self.name, self.description, &self.params)
    }

    // pub fn get_default_values(&self) -> [Value; N] {
    //     core::array::from_fn(|i| {
    //         if i < self.len {
    //             self.params[i].default()
    //         } else {
    //             Value::None
    //         }
    //     })
    // }
}

pub struct RuntimeConfig<const N: usize> {
    len: usize,
    values: [Value; N],
    default_values: [Value; N],
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidIndex,
    TypeMismatch,
    EepromError,
}

impl<const N: usize> RuntimeConfig<N> {
    // Get all values
    pub fn values(&self) -> &[Value] {
        &self.values[0..self.len]
    }

    // Get a specific value by index
    pub fn value(&self, index: usize) -> Option<&Value> {
        if index < self.len {
            Some(&self.values[index])
        } else {
            None
        }
    }

    // Set a specific value by index
    pub fn set_value(&mut self, index: usize, value: Value) -> Result<(), ConfigError> {
        if index >= self.len {
            return Err(ConfigError::InvalidIndex);
        }

        // Type checking - make sure the value type matches the existing value type
        if !value_types_match(&self.values[index], &value) {
            return Err(ConfigError::TypeMismatch);
        }

        self.values[index] = value;
        Ok(())
    }

    // Reset a value to its default
    pub fn reset_value(&mut self, index: usize) -> Result<(), ConfigError> {
        if index >= self.len {
            return Err(ConfigError::InvalidIndex);
        }

        self.values[index] = self.default_values[index];
        Ok(())
    }

    // Reset all values to defaults
    pub fn reset_all_values(&mut self) {
        for i in 0..self.len {
            self.values[i] = self.default_values[i];
        }
    }

    // Helper methods for common value types - return default values instead of Options
}

// Helper function to check if two values are of the same type
fn value_types_match(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::None, Value::None) => true,
        (Value::Int(_), Value::Int(_)) => true,
        (Value::Float(_), Value::Float(_)) => true,
        (Value::Bool(_), Value::Bool(_)) => true,
        (Value::Enum(_), Value::Enum(_)) => true,
        (Value::Curve(_), Value::Curve(_)) => true,
        (Value::Waveform(_), Value::Waveform(_)) => true,
        _ => false,
    }
}
