pub mod buttons;
pub mod calibration;
pub mod clock;
#[cfg_attr(feature = "midi-only", allow(dead_code))]
pub mod configure;
pub mod fram;
pub mod global_config;
pub mod i2c;
pub mod input_handlers;
pub mod leds;
pub mod max;
pub mod midi;
pub mod transport;
#[cfg_attr(feature = "midi-only", allow(dead_code))]
pub mod web_usb;
