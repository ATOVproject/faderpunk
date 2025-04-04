#![no_std]

use postcard_bindgen::PostcardBindings;
use serde::{Deserialize, Serialize};

use libfp::constants::{WAVEFORM_RECT, WAVEFORM_SAW, WAVEFORM_SINE, WAVEFORM_TRIANGLE};

/// Maximum number of params per app
pub const MAX_PARAMS: usize = 16;

#[derive(Clone, Copy, PartialEq)]
pub enum ClockSrc {
    None,
    Atom,
    Meteor,
    Cube,
    Internal,
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

#[derive(Clone, Copy, Serialize, Deserialize, PostcardBindings)]
pub enum Curve {
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
        default: i32,
        min: usize,
        max: usize,
    },
    Float {
        name: &'static str,
        default: f32,
    },
    Bool {
        name: &'static str,
        default: bool,
    },
    Enum {
        name: &'static str,
        default: usize,
        variants: &'static [&'static str],
    },
    Curve {
        name: &'static str,
        default: Curve,
        variants: &'static [Curve],
    },
    Waveform {
        name: &'static str,
        default: Waveform,
        variants: &'static [Waveform],
    },
}

impl Param {
    fn default(&self) -> Value {
        match &self {
            Param::None => Value::None,
            Param::Int { default, .. } => Value::Int(*default),
            Param::Float { default, .. } => Value::Float(*default),
            Param::Bool { default, .. } => Value::Bool(*default),
            Param::Curve { default, .. } => Value::Curve(*default),
            Param::Waveform { default, .. } => Value::Waveform(*default),
            Param::Enum { default, .. } => Value::Enum(*default),
        }
    }
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

    // Create a function that returns a RuntimeConfig with values from EEPROM
    pub async fn as_runtime_config(&self) -> RuntimeConfig<N> {
        // TODO: Read stored values from EEPROM
        let stored_values = [Value::None; 4];

        // Create default values
        let default_values = core::array::from_fn(|i| {
            if i < self.len {
                self.params[i].default()
            } else {
                Value::None
            }
        });

        // Merge stored values with defaults
        let values = core::array::from_fn(|i| {
            if i < self.len {
                match stored_values[i] {
                    Value::None => default_values[i],
                    _ => stored_values[i],
                }
            } else {
                Value::None
            }
        });

        RuntimeConfig {
            len: self.len,
            values,
            default_values,
        }
    }
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
