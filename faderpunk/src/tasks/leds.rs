use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::Timer;
use libfp::{constants::CHAN_LED_MAP, ext::BrightnessExt};
use smart_leds::colors::BLACK;
use smart_leds::{gamma, SmartLedsWriteAsync, RGB8};
use ws2812_async::{Grb, Ws2812};
use {defmt_rtt as _, panic_probe as _};

const REFRESH_RATE: u64 = 60;
const T: u64 = 1000 / REFRESH_RATE;
const NUM_LEDS: usize = 50;
const LED_CHANNEL_SIZE: usize = 16;

pub type LedSender = Sender<'static, CriticalSectionRawMutex, LedMsg, LED_CHANNEL_SIZE>;
pub static LED_CHANNEL: Channel<CriticalSectionRawMutex, LedMsg, LED_CHANNEL_SIZE> = Channel::new();

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, SPI1, Async>) {
    spawner.spawn(run_leds(spi1)).unwrap();
}

pub enum LedMsg {
    Reset(usize, Led),
    ResetAll(usize),
    Set(usize, Led, LedMode),
}

pub enum Led {
    Top,
    Bottom,
    Button,
}

pub enum LedMode {
    Static(RGB8),
    FadeOut(RGB8),
}

impl LedMode {
    fn into_effect(self) -> LedEffect {
        match self {
            LedMode::Static(color) => LedEffect::Static { color },
            LedMode::FadeOut(from) => LedEffect::FadeOut { from, step: 0 },
        }
    }
}

#[derive(Clone, Copy)]
enum LedEffect {
    Off,
    Static { color: RGB8 },
    FadeOut { from: RGB8, step: u8 },
}

impl LedEffect {
    fn update(&mut self) -> RGB8 {
        match self {
            LedEffect::Off => BLACK,
            LedEffect::Static { color } => *color,
            LedEffect::FadeOut { from, step } => {
                let new_color = from.scale(255 - *step);
                if *step < 255 {
                    *step = step.saturating_add(32);
                } else {
                    *self = LedEffect::Off;
                }
                new_color
            }
        }
    }
}

fn get_no(channel: usize, position: Led) -> usize {
    match position {
        Led::Top => CHAN_LED_MAP[0][channel],
        Led::Bottom => CHAN_LED_MAP[1][channel],
        Led::Button => CHAN_LED_MAP[2][channel],
    }
}

#[embassy_executor::task]
async fn run_leds(spi1: Spi<'static, SPI1, Async>) {
    let mut ws: Ws2812<_, Grb, { 12 * NUM_LEDS }> = Ws2812::new(spi1);

    let mut led_state = [LedEffect::Off; 50];
    let mut buffer = [BLACK; NUM_LEDS];

    led_state[16] = LedEffect::Static {
        color: RGB8 {
            r: 75,
            g: 75,
            b: 75,
        },
    };
    led_state[17] = LedEffect::Static {
        color: RGB8 {
            r: 75,
            g: 75,
            b: 75,
        },
    };

    loop {
        match select(Timer::after_millis(T), LED_CHANNEL.receive()).await {
            Either::First(_) => {}
            Either::Second(msg) => match msg {
                LedMsg::Set(chan, pos, mode) => {
                    led_state[get_no(chan, pos)] = mode.into_effect();
                }
                LedMsg::Reset(chan, pos) => {
                    let index = get_no(chan, pos);
                    if let LedEffect::Static { color } = led_state[index] {
                        led_state[index] = LedMode::FadeOut(color).into_effect();
                    }
                }
                LedMsg::ResetAll(chan) => {
                    for pos in [Led::Top, Led::Bottom, Led::Button] {
                        let index = get_no(chan, pos);
                        if let LedEffect::Static { color } = led_state[index] {
                            led_state[index] = LedMode::FadeOut(color).into_effect();
                        }
                    }
                }
            },
        }

        for (effect, led) in led_state.iter_mut().zip(buffer.iter_mut()) {
            *led = effect.update();
        }
        ws.write(gamma(buffer.iter().cloned())).await.ok();
    }
}
