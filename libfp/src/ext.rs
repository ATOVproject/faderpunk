use smart_leds::RGB8;

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
