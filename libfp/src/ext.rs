use smart_leds::RGB8;

use crate::Value;

pub trait BrightnessExt {
    /// Scales the color by a brightness factor.
    /// A brightness of 255 means full intensity, 0 means black.
    fn scale(&self, brightness: u8) -> Self;
}

impl BrightnessExt for RGB8 {
    fn scale(&self, brightness: u8) -> Self {
        let r = ((self.r as u16 * (brightness as u16 + 1)) >> 8) as u8;
        let g = ((self.g as u16 * (brightness as u16 + 1)) >> 8) as u8;
        let b = ((self.b as u16 * (brightness as u16 + 1)) >> 8) as u8;
        Self { r, g, b }
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
