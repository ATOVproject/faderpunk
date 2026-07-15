//! WS2812B LED strip driver: renders fp-core's LED effect engine over SPI at
//! the fixed refresh rate, plus the boot animation.

use embassy_executor::Spawner;
use embassy_rp::clocks::RoscRng;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_time::{Duration, Instant, Timer};
use libfp::{Brightness, Color};
use portable_atomic::Ordering;
use smart_leds::colors::BLACK;
use smart_leds::{brightness, gamma, SmartLedsWriteAsync, RGB8};
use ws2812_async::{Grb, Ws2812};

use fp_core::tasks::leds::{LedMode, LedProcessor, LED_BRIGHTNESS, NUM_LEDS, T};

type Ws = Ws2812<Spi<'static, SPI1, Async>, Grb, { 12 * NUM_LEDS }>;

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, SPI1, Async>) {
    spawner.spawn(run_leds(spi1)).unwrap();
}

async fn flush_buffer(ws: &mut Ws, buffer: &[RGB8; NUM_LEDS]) {
    ws.write(gamma(brightness(
        buffer.iter().cloned(),
        LED_BRIGHTNESS.load(Ordering::Relaxed),
    )))
    .await
    .ok();
}

#[embassy_executor::task]
async fn run_leds(spi1: Spi<'static, SPI1, Async>) {
    let mut ws: Ws = Ws2812::new(spi1);

    startup_animation(&mut ws).await;

    let mut leds = LedProcessor::new();

    leds.set_base_mode(
        16,
        LedMode::ClockFlash(Color::Pink, Brightness::High, Brightness::Mid),
    );
    leds.set_base_mode(17, LedMode::Static(Color::Yellow, Brightness::Mid));

    loop {
        // Wait for the next frame
        Timer::after_millis(T).await;

        leds.poll_messages();
        let buffer = *leds.render();
        flush_buffer(&mut ws, &buffer).await;
    }
}

async fn startup_animation(ws: &mut Ws) {
    let palette: [RGB8; 3] = [Color::Yellow.into(), Color::Cyan.into(), Color::Pink.into()];
    let mut buffer: [RGB8; NUM_LEDS] = [BLACK; NUM_LEDS];

    // Glitchy flashes
    let start_time = Instant::now();
    let animation_duration = Duration::from_millis(1500);

    while Instant::now().duration_since(start_time) < animation_duration {
        // 10% chance for a full-strip flash as the base layer
        if RoscRng::next_u8() < 26 {
            let flash_color_idx = (RoscRng::next_u8() as usize) % palette.len();
            buffer.fill(palette[flash_color_idx]);
        } else {
            // Otherwise, start with a black background
            buffer.fill(BLACK);
        }

        // Layer 2 to 5 "glitch events" on top
        let num_events = 2 + (RoscRng::next_u8() % 4);
        for _ in 0..num_events {
            let event_type = RoscRng::next_u8();
            let start = (RoscRng::next_u8() as usize) % NUM_LEDS;
            let max_len = (NUM_LEDS / 2).max(1);
            let len = 1 + (RoscRng::next_u8() as usize) % max_len;

            // ~65% chance of a colored glitch
            if event_type < 166 {
                let color_idx = (RoscRng::next_u8() as usize) % palette.len();
                let color = palette[color_idx];
                for led in &mut buffer[start..(start + len).min(NUM_LEDS)] {
                    *led = color;
                }
            } else {
                // ~35% chance of a white static glitch
                for led in &mut buffer[start..(start + len).min(NUM_LEDS)] {
                    let val = 128 + (RoscRng::next_u8() % 128);
                    *led = RGB8 {
                        r: val,
                        g: val,
                        b: val,
                    };
                }
            }
        }

        flush_buffer(ws, &buffer).await;
        Timer::after_millis(100).await;
    }

    // Color sweep
    for i in 0..NUM_LEDS {
        buffer[i] = palette[i % palette.len()];
        if i > 0 {
            buffer[i - 1] = BLACK;
        }
        flush_buffer(ws, &buffer).await;
        Timer::after_millis(15).await;
    }
    // Clear last LED
    buffer[NUM_LEDS - 1] = BLACK;
    flush_buffer(ws, &buffer).await;
    Timer::after_millis(250).await;

    // Final Flash
    buffer.fill(Color::Pink.into());
    flush_buffer(ws, &buffer).await;
    Timer::after_millis(100).await;

    // Fade to black
    let pink: RGB8 = Color::Pink.into();
    for i in (0..=255).rev().step_by(8) {
        use libfp::ext::BrightnessExt;
        let scaled_color = pink.scale(i);
        buffer.fill(scaled_color);
        flush_buffer(ws, &buffer).await;
        Timer::after_millis(T).await;
    }

    buffer.fill(BLACK);
    flush_buffer(ws, &buffer).await;
}
