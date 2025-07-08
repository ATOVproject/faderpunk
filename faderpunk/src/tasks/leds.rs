use embassy_executor::Spawner;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use libfp::{constants::CHAN_LED_MAP, ext::BrightnessExt};
use smart_leds::colors::BLACK;
use smart_leds::{gamma, SmartLedsWriteAsync, RGB8};
use ws2812_async::{Grb, Ws2812};

const REFRESH_RATE: u64 = 60;
const T: u64 = 1000 / REFRESH_RATE;
const NUM_LEDS: usize = 50;
const LED_OVERLAY_CHANNEL_SIZE: usize = 16;

static LED_SIGNALS: [Signal<CriticalSectionRawMutex, LedMsg>; NUM_LEDS] =
    [const { Signal::new() }; NUM_LEDS];

static LED_OVERLAY_CHANNEL: Channel<
    CriticalSectionRawMutex,
    (usize, LedMode),
    LED_OVERLAY_CHANNEL_SIZE,
> = Channel::new();

pub async fn start_leds(spawner: &Spawner, spi1: Spi<'static, SPI1, Async>) {
    spawner.spawn(run_leds(spi1)).unwrap();
}

#[derive(Clone, Copy)]
pub enum LedMsg {
    Reset,
    Set(LedMode),
}

#[derive(Clone, Copy)]
pub enum Led {
    Top,
    Bottom,
    Button,
}

#[derive(Clone, Copy)]
pub enum LedMode {
    Static(RGB8),
    FadeOut(RGB8),
    Flash(RGB8, usize),
}

impl LedMode {
    fn into_effect(self) -> LedEffect {
        match self {
            LedMode::Static(color) => LedEffect::Static { color },
            LedMode::FadeOut(from) => LedEffect::FadeOut { from, step: 0 },
            LedMode::Flash(color, times) => LedEffect::Flash {
                color,
                times,
                step: 0,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum LedEffect {
    Off,
    Static { color: RGB8 },
    FadeOut { from: RGB8, step: u8 },
    Flash { color: RGB8, times: usize, step: u8 },
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
            LedEffect::Flash { color, times, step } => {
                if *times == 0 {
                    *self = LedEffect::Off;
                    return BLACK;
                }

                // Each flash cycle has 16 steps total (8 on, 8 off)
                let cycle_step = *step % 16;
                let result = if cycle_step < 8 {
                    // First 8 steps: fade out from full brightness (sawtooth)
                    let fade_step = cycle_step * 32;
                    color.scale(255 - fade_step)
                } else {
                    // Next 8 steps: off
                    BLACK
                };

                *step += 1;
                if *step >= 16 {
                    *times -= 1;
                    *step = 0;
                    if *times == 0 {
                        *self = LedEffect::Off;
                    }
                }

                result
            }
        }
    }
}

struct LedProcessor {
    base_layer: [LedEffect; 50],
    overlay_layer: [LedEffect; 50],
    buffer: [RGB8; NUM_LEDS],
    ws: Ws2812<Spi<'static, SPI1, Async>, Grb, { 12 * NUM_LEDS }>,
}

impl LedProcessor {
    async fn process(&mut self) {
        for ((base, overlay), led) in self
            .base_layer
            .iter_mut()
            .zip(self.overlay_layer.iter_mut())
            .zip(self.buffer.iter_mut())
        {
            if let LedEffect::Off = overlay {
                // Overlay effect is off, we use the base layer
                *led = base.update();
            } else {
                // Overlay effect is present, use that
                *led = overlay.update();
                match base {
                    LedEffect::Off | LedEffect::Static { .. } => {
                        // Off and Static are stateless, no update needed
                    }
                    _ => {
                        // Also update base layer to continue effects that have state
                        base.update();
                    }
                }
            }
        }
        self.ws.write(gamma(self.buffer.iter().cloned())).await.ok();
    }
}

fn get_no(channel: usize, position: Led) -> usize {
    match position {
        Led::Top => CHAN_LED_MAP[0][channel],
        Led::Bottom => CHAN_LED_MAP[1][channel],
        Led::Button => CHAN_LED_MAP[2][channel],
    }
}

pub fn set_led_mode(channel: usize, position: Led, msg: LedMsg) {
    let no = get_no(channel, position);
    LED_SIGNALS[no].signal(msg);
}

pub async fn set_led_overlay_mode(channel: usize, position: Led, mode: LedMode) {
    let no = get_no(channel, position);
    LED_OVERLAY_CHANNEL.send((no, mode)).await;
}

#[embassy_executor::task]
async fn run_leds(spi1: Spi<'static, SPI1, Async>) {
    let ws: Ws2812<_, Grb, { 12 * NUM_LEDS }> = Ws2812::new(spi1);

    let mut leds = LedProcessor {
        base_layer: [LedEffect::Off; NUM_LEDS],
        overlay_layer: [LedEffect::Off; NUM_LEDS],
        buffer: [BLACK; NUM_LEDS],
        ws,
    };

    // TODO: find a better way to initialize these
    leds.base_layer[16] = LedEffect::Static {
        color: RGB8 {
            r: 75,
            g: 75,
            b: 75,
        },
    };
    leds.base_layer[17] = LedEffect::Static {
        color: RGB8 {
            r: 75,
            g: 75,
            b: 75,
        },
    };

    loop {
        // Wait for the next frame
        Timer::after_millis(T).await;

        // Check all signals for new messages
        for (i, led_signal) in LED_SIGNALS.iter().enumerate() {
            if let Some(msg) = led_signal.try_take() {
                match msg {
                    LedMsg::Set(mode) => {
                        leds.base_layer[i] = mode.into_effect();
                    }
                    LedMsg::Reset => {
                        if let LedEffect::Static { color } = leds.base_layer[i] {
                            leds.base_layer[i] = LedMode::FadeOut(color).into_effect();
                        }
                    }
                }
            }
        }

        while let Ok((no, mode)) = LED_OVERLAY_CHANNEL.try_receive() {
            leds.overlay_layer[no] = mode.into_effect();
        }

        leds.process().await;
    }
}
