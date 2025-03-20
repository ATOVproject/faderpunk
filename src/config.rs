use minicbor::Encode;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq)]
pub enum ClockSrc {
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
            reset_src: ClockSrc::Internal,
            layout: &[1; 16],
        }
    }
}

#[derive(Clone, Copy, Encode)]
pub enum Curve {
    #[n(0)]
    Linear,
    #[n(1)]
    Exponential,
    #[n(2)]
    Logarithmic,
}

#[derive(Clone, Copy, Encode)]
pub enum Waveform {
    #[n(0)]
    Sine,
    #[n(1)]
    Rect,
    #[n(2)]
    Triangle,
    #[n(3)]
    Saw,
}

#[derive(Encode)]
pub enum Param {
    #[n(0)]
    None,
    #[n(1)]
    Int {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: i32,
    },
    #[n(2)]
    Float {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: f32,
    },
    #[n(3)]
    Bool {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: bool,
    },
    #[n(4)]
    Enum {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: usize,
        #[n(2)]
        variants: &'static [&'static str],
    },
    #[n(5)]
    Curve {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: Curve,
        #[n(2)]
        variants: &'static [Curve],
    },
    #[n(6)]
    Waveform {
        #[n(0)]
        name: &'static str,
        #[n(1)]
        default: Waveform,
        #[n(2)]
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

// TODO: Encode with postcard (https://github.com/jamesmunns/postcard)
#[derive(Clone, Copy)]
pub enum Value {
    None,
    Int(i32),
    Float(f32),
    Bool(bool),
    Enum(usize),
    Curve(Curve),
    Waveform(Waveform),
}

#[derive(Serialize, Deserialize)]
pub enum ConfigureMessage {
    GetApps,
}

pub struct Config<const N: usize> {
    len: usize,
    params: [Param; N],
}

impl<const N: usize> Config<N> {
    pub const fn default() -> Self {
        Config {
            len: 0,
            params: [const { Param::None }; N],
        }
    }

    pub const fn add_param(mut self, param: Param) -> Self {
        self.params[self.len] = param;
        let new_len = self.len + 1;
        Config {
            len: new_len,
            params: self.params,
        }
    }

    // Create a function that returns a RuntimeConfig with values from EEPROM
    pub async fn to_runtime_config(&self) -> RuntimeConfig<N> {
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
