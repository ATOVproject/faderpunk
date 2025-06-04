use embassy_executor::Spawner;
use embassy_rp::clocks::RoscRng;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_time::{Duration, Instant, Timer};
use portable_atomic::{AtomicU32, Ordering};
use rand::Rng;
use smart_leds::colors::{BLACK, CYAN, MAGENTA, YELLOW};
use smart_leds::{brightness, gamma, SmartLedsWriteAsync, RGB8};
use ws2812_async::{Grb, Ws2812};
use {defmt_rtt as _, panic_probe as _};

// TODO: Add snappy fade out for all LEDs when turning off

const REFRESH_RATE: u64 = 60;
const NUM_LEDS: usize = 50;
pub static LED_VALUES: [AtomicU32; NUM_LEDS] = [const { AtomicU32::new(0) }; NUM_LEDS];

#[derive(Clone, Copy)]
pub enum LedsAction {
    Idle,
    Flush,
    Blink(u64),
}

impl LedsAction {
    fn get_value(&self) -> u8 {
        match self {
            Self::Blink(_) => 255,
            Self::Idle => 0,
            Self::Flush => 0,
        }
    }
}

type XRxReceiver = Receiver<'static, NoopRawMutex, (usize, LedsAction), 64>;

#[inline(always)]
pub fn decode_val(value: u32) -> RGB8 {
    // TODO: Add global max led brightness
    let brightness = ((value >> 24) & 0xFF) as u16;
    RGB8 {
        r: (((value >> 16) & 0xFF) as u16 * (brightness + 1) / 256) as u8,
        g: (((value >> 8) & 0xFF) as u16 * (brightness + 1) / 256) as u8,
        b: ((value & 0xFF) as u16 * (brightness + 1) / 256) as u8,
    }
}

async fn run_leds_startup(ws: &mut Ws2812<Spi<'static, SPI1, Async>, Grb, { 12 * NUM_LEDS }>) {
    let mut data = [BLACK; NUM_LEDS];
    const BRIGHTNESS: u8 = 75; // Increase brightness a bit for impact? (0-255)
    const FRAME_DELAY_MS: u64 = 100; // Speed of glitching (lower is faster)
    const STARTUP_DURATION_SECS: u64 = 2; // Duration of the effect

    const PALETTE: [RGB8; 3] = [CYAN, MAGENTA, YELLOW];
    let mut rng = RoscRng;

    let start_time = Instant::now();
    let animation_duration = Duration::from_secs(STARTUP_DURATION_SECS);

    loop {
        if start_time.elapsed() >= animation_duration {
            break;
        }

        let base_chance = rng.gen_range(0..100);
        if base_chance < 10 {
            let flash_color = PALETTE[rng.gen_range(0..PALETTE.len() as u32) as usize];
            for i in 0..NUM_LEDS {
                data[i] = flash_color;
            }
        } else {
            for i in 0..NUM_LEDS {
                data[i] = BLACK;
            }
        }

        let num_events = 2 + rng.gen_range(0..4);

        for _ in 0..num_events {
            let event_type = rng.gen_range(0..100);

            let start = rng.gen_range(0..NUM_LEDS as u32) as usize;
            let max_len = (NUM_LEDS / 2).max(1);
            let len = 1 + rng.gen_range(0..max_len as u32);

            if event_type < 65 {
                let color_idx = rng.gen_range(0..PALETTE.len() as u32) as usize;
                let color = PALETTE[color_idx];
                for i in start..(start + len as usize).min(NUM_LEDS) {
                    data[i] = color;
                }
            } else {
                for i in start..(start + len as usize).min(NUM_LEDS) {
                    let val = (128 + rng.gen_range(0..128)) as u8;
                    data[i] = RGB8 {
                        r: val,
                        g: val,
                        b: val,
                    };
                }
            }
        }

        ws.write(brightness(data.iter().cloned(), BRIGHTNESS))
            .await
            .ok();

        Timer::after_millis(FRAME_DELAY_MS).await;
    }

    let final_data = [BLACK; NUM_LEDS];
    ws.write(final_data.iter().cloned()).await.ok();
}

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, SPI1, Async>) {
    spawner.spawn(run_leds(spi1)).unwrap();
}

#[embassy_executor::task]
// TODO: Implement effects (using a channel)
async fn run_leds(spi1: Spi<'static, SPI1, Async>) {
    let mut ws: Ws2812<_, Grb, { 12 * NUM_LEDS }> = Ws2812::new(spi1);
    let delta = 1000 / REFRESH_RATE;

    run_leds_startup(&mut ws).await;

    // White at 75 brightness
    LED_VALUES[16].store(1275068415, Ordering::Relaxed);
    LED_VALUES[17].store(1275068415, Ordering::Relaxed);

    loop {
        let data = gamma(
            LED_VALUES
                .iter()
                .map(|val| decode_val(val.load(Ordering::Relaxed))),
        );

        ws.write(data).await.ok();
        Timer::after_millis(delta).await;
    }
}
