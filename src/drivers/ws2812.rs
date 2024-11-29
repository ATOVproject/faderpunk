use embedded_hal_async::spi::{ErrorType, SpiBus};
use smart_leds::RGB8;

const PATTERNS: [u8; 4] = [0b1000_1000, 0b1000_1110, 0b1110_1000, 0b1110_1110];

/// Trait for color order reordering
pub trait OrderedColors {
    fn reorder(color: RGB8) -> [u8; 3];
}

/// Marker struct for RGB order
pub struct Rgb;

/// Marker struct for GRB order
pub struct Grb;

/// RGB order implementation
impl OrderedColors for Rgb {
    fn reorder(color: RGB8) -> [u8; 3] {
        [color.r, color.g, color.b]
    }
}

/// GRB order implementation
impl OrderedColors for Grb {
    fn reorder(color: RGB8) -> [u8; 3] {
        [color.g, color.r, color.b]
    }
}

/// N = 12 * NUM_LEDS
pub struct Ws2812<SPI: SpiBus<u8>, C: OrderedColors, const N: usize> {
    spi: SPI,
    data: [u8; N],
    color_order: C,
}

impl<SPI: SpiBus<u8>, C: OrderedColors, const N: usize> Ws2812<SPI, C, N> {
    /// Create a new WS2812 driver, with the given SPI bus
    pub fn new(spi: SPI, color_order: C) -> Self {
        Self {
            spi,
            data: [0; N],
            color_order,
        }
    }

    pub async fn write(
        &mut self,
        iter: impl Iterator<Item = RGB8>,
    ) -> Result<(), <SPI as ErrorType>::Error> {
        for (led_bytes, rgb8) in self.data.chunks_mut(12).zip(iter) {
            let colors = C::reorder(rgb8);
            for (i, mut color) in colors.into_iter().enumerate() {
                for ii in 0..4 {
                    led_bytes[i * 4 + ii] = PATTERNS[((color & 0b1100_0000) >> 6) as usize];
                    color <<= 2;
                }
            }
        }
        self.spi.write(&self.data).await?;
        let blank = [0_u8; 140];
        self.spi.write(&blank).await
    }
}
