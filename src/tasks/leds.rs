use crate::drivers::ws2812::{Grb, Ws2812};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{self, Spi};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex};
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use smart_leds::{brightness, RGB8};
use {defmt_rtt as _, panic_probe as _};

// FIXME: DOUBLE CHECK ALL CHANNELS and maybe use ATOMICS if possible as we have 16 apps
// that can potentially lock up a channel
// FIXME: Add snappy fade out for all LEDs when turning off

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

pub static CHANNEL_LEDS: Channel<CriticalSectionRawMutex, (usize, LedsAction), 16> = Channel::new();

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
    // let data = [RGB8 {
    //     r: 255,
    //     g: 10,
    //     b: 10,
    // }; 18];
    // ws.write(brightness(data.iter().cloned(), 32))
    //     .await
    //     .unwrap();

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
    // let mut led_driver = Is31Fl3218::new(i2c_device);
    //
    // let refresh_millis = 1000 / REFRESH_RATE;
    //
    // led_driver.enable_device().await.unwrap();
    // led_driver.enable_all().await.unwrap();
    //
    // let led_state: Mutex<NoopRawMutex, [LedsAction; 18]> = Mutex::new([LedsAction::Idle; 18]);
    // // FIXME: we need a prev_state to be able to override a running effect/action and then go back
    // // to the previous one instead of idle
    //
    // let display_fut = async {
    //     let mut led_buf: [u8; 18] = [0; 18];
    //     let mut changed = false;
    //     loop {
    //         let mut state = led_state.lock().await;
    //         for (i, led) in state.iter_mut().enumerate() {
    //             match &led {
    //                 // FIXME: this could maybe be in the LedsAction implementation?
    //                 LedsAction::Blink(duration) => {
    //                     let next_duration = duration.saturating_sub(refresh_millis);
    //                     *led = if next_duration == 0 {
    //                         LedsAction::Idle
    //                     } else {
    //                         LedsAction::Blink(next_duration)
    //                     };
    //                     changed = true;
    //                 }
    //                 LedsAction::Idle => {}
    //             }
    //             led_buf[i] = led.get_value();
    //         }
    //         drop(state);
    //         if changed {
    //             led_driver.set_all(&led_buf).await.unwrap();
    //             changed = false;
    //         }
    //         // Roughly 60Hz refresh rate
    //         Timer::after_millis(refresh_millis).await;
    //     }
    // };
    //
    // let mod_fut = async {
    //     loop {
    //         let (chan, action) = CHANNEL_LEDS.receive().await;
    //         let mut state = led_state.lock().await;
    //         state[chan] = action;
    //     }
    // };
    //
    // join(display_fut, mod_fut).await;
}
