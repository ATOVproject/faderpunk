use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_time::Timer;
use esp_hal::{spi::master::Spi, Async};
use portable_atomic::{AtomicU32, Ordering};
use smart_leds_trait::{SmartLedsWriteAsync, RGB8};
use ws2812_async::{Grb, Ws2812};

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
    let brightness = ((value >> 24) & 0xFF) as u16;
    RGB8 {
        r: (((value >> 16) & 0xFF) as u16 * (brightness + 1) / 256) as u8,
        g: (((value >> 8) & 0xFF) as u16 * (brightness + 1) / 256) as u8,
        b: ((value & 0xFF) as u16 * (brightness + 1) / 256) as u8,
    }
}

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, Async>, x_rx: XRxReceiver) {
    spawner.spawn(run_leds(spi1, x_rx)).unwrap();
}

#[embassy_executor::task]
async fn run_leds(spi1: Spi<'static, Async>, x_rx: XRxReceiver) {
    let mut ws: Ws2812<_, Grb, { 12 * NUM_LEDS }> = Ws2812::new(spi1);

    loop {
        // TODO: match for effects and flush
        let _ = x_rx.receive().await;

        let data = LED_VALUES
            .iter()
            .map(|val| decode_val(val.load(Ordering::Relaxed)));

        ws.write(data).await.ok();
        Timer::after_millis(5).await;
    }
}
