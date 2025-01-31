use crate::drivers::ws2812::{Grb, Ws2812};
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{self, Spi};
use embassy_time::Timer;
use smart_leds::{brightness, RGB8};
use {defmt_rtt as _, panic_probe as _};

// TODO: Add snappy fade out for all LEDs when turning off

const REFRESH_RATE: u64 = 60;
const NUM_LEDS: usize = 50;

#[derive(Clone, Copy)]
pub enum LedsAction {
    Idle,
    Blink(u64),
}

impl LedsAction {
    fn get_value(&self) -> u8 {
        match self {
            Self::Blink(_) => 255,
            Self::Idle => 0,
        }
    }
}

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, SPI1, spi::Async>) {
    spawner.spawn(run_leds(spi1)).unwrap();
}

fn wheel(mut wheel_pos: u8) -> RGB8 {
    wheel_pos = 255 - wheel_pos;
    if wheel_pos < 85 {
        return (255 - wheel_pos * 3, 0, wheel_pos * 3).into();
    }
    if wheel_pos < 170 {
        wheel_pos -= 85;
        return (0, wheel_pos * 3, 255 - wheel_pos * 3).into();
    }
    wheel_pos -= 170;
    (wheel_pos * 3, 255 - wheel_pos * 3, 0).into()
}

#[embassy_executor::task]
async fn run_leds(spi1: Spi<'static, SPI1, spi::Async>) {
    let mut ws: Ws2812<_, Grb, { 12 * NUM_LEDS }> = Ws2812::new(spi1);

    let mut data = [RGB8::default(); NUM_LEDS];

    loop {
        for j in 0..(256 * 5) {
            for i in 0..NUM_LEDS {
                data[i] = wheel((((i * 256) as u16 / NUM_LEDS as u16 + j as u16) & 255) as u8);
            }
            ws.write(brightness(data.iter().cloned(), 32)).await.ok();
            Timer::after_millis(5).await;
        }
    }
}
