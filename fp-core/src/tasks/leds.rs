//! LED state and effect engine. Apps set per-LED modes through signals; the
//! host's frame loop (WS2812 over SPI on hardware, the panel UI on the
//! simulator) polls messages and renders frames at [`REFRESH_RATE`] via
//! [`LedProcessor`].

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use libfp::constants::CHAN_LED_MAP;
use libfp::ext::BrightnessExt;
use libfp::{Brightness, Color, LED_BRIGHTNESS_RANGE};
use portable_atomic::{AtomicU8, Ordering};
use smart_leds::colors::BLACK;
use smart_leds::RGB8;

use crate::tasks::clock::METRONOME_HIGH;

pub const REFRESH_RATE: u64 = 60;
pub const T: u64 = 1000 / REFRESH_RATE;
pub const NUM_LEDS: usize = 50;
const LED_OVERLAY_CHANNEL_SIZE: usize = 16;

pub static LED_BRIGHTNESS: AtomicU8 = AtomicU8::new(LED_BRIGHTNESS_RANGE.end);

static LED_SIGNALS: [Signal<CriticalSectionRawMutex, LedMsg>; NUM_LEDS] =
    [const { Signal::new() }; NUM_LEDS];

static LED_OVERLAY_CHANNEL: Channel<
    CriticalSectionRawMutex,
    (usize, Option<LedMode>),
    LED_OVERLAY_CHANNEL_SIZE,
> = Channel::new();

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
#[allow(dead_code)]
pub enum LedMode {
    Static(Color, Brightness),
    FadeOut(Color),
    Flash(Color, Option<usize>),
    StaticFade(Color, u16),
    ClockFlash(Color, Brightness, Brightness),
    FlashThenStatic(Color, usize, Color, Brightness),
}

impl LedMode {
    fn into_effect(self) -> LedEffect {
        match self {
            LedMode::Static(color, brightness) => LedEffect::Static {
                color: color.into(),
                brightness: brightness.into(),
            },
            LedMode::FadeOut(from) => LedEffect::FadeOut {
                from: from.into(),
                step: 0,
            },
            LedMode::Flash(color, times) => LedEffect::Flash {
                color: color.into(),
                times,
                step: 0,
            },
            LedMode::StaticFade(color, delay_ms) => LedEffect::StaticFade {
                color: color.into(),
                delay_ms,
                elapsed_frames: 0,
            },
            LedMode::ClockFlash(color, brightness_high, brightness_low) => LedEffect::ClockFlash {
                color: color.into(),
                brightness_high: brightness_high.into(),
                brightness_low: brightness_low.into(),
            },
            LedMode::FlashThenStatic(color, times, then_color, then_brightness) => {
                LedEffect::FlashThenStatic {
                    color: color.into(),
                    times,
                    step: 0,
                    then_color: then_color.into(),
                    then_brightness: then_brightness.into(),
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum LedEffect {
    Off,
    Static {
        color: RGB8,
        brightness: u8,
    },
    FadeOut {
        from: RGB8,
        step: u8,
    },
    Flash {
        color: RGB8,
        times: Option<usize>,
        step: u8,
    },
    StaticFade {
        color: RGB8,
        delay_ms: u16,
        elapsed_frames: u64,
    },
    ClockFlash {
        color: RGB8,
        brightness_high: u8,
        brightness_low: u8,
    },
    FlashThenStatic {
        color: RGB8,
        times: usize,
        step: u8,
        then_color: RGB8,
        then_brightness: u8,
    },
}

impl LedEffect {
    fn update(&mut self) -> RGB8 {
        match self {
            LedEffect::Off => BLACK,
            LedEffect::Static { color, brightness } => {
                if *brightness == 255 {
                    *color
                } else {
                    color.scale(*brightness)
                }
            }
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
                if let Some(0) = *times {
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
                    if let Some(t) = times {
                        *t -= 1;
                        *step = 0;
                        if *t == 0 {
                            *self = LedEffect::Off;
                        }
                    } else {
                        *step = 0;
                    }
                }

                result
            }
            LedEffect::ClockFlash {
                color,
                brightness_high,
                brightness_low,
            } => {
                if METRONOME_HIGH.load(Ordering::Relaxed) {
                    color.scale(*brightness_high)
                } else {
                    color.scale(*brightness_low)
                }
            }
            LedEffect::FlashThenStatic {
                color,
                times,
                step,
                then_color,
                then_brightness,
            } => {
                if *times == 0 {
                    let c = *then_color;
                    let b = *then_brightness;
                    *self = LedEffect::Static {
                        color: c,
                        brightness: b,
                    };
                    return c.scale(b);
                }

                let cycle_step = *step % 16;
                let result = if cycle_step < 8 {
                    let fade_step = cycle_step * 32;
                    color.scale(255 - fade_step)
                } else {
                    BLACK
                };

                *step += 1;
                if *step >= 16 {
                    *times -= 1;
                    *step = 0;
                }

                result
            }
            LedEffect::StaticFade {
                color,
                delay_ms,
                elapsed_frames,
            } => {
                // Calculate elapsed time in milliseconds based on frames
                let elapsed_ms = *elapsed_frames * T;

                if elapsed_ms >= *delay_ms as u64 {
                    let color = *color;
                    // Time has elapsed, transition to fade out
                    *self = LedEffect::FadeOut {
                        from: color,
                        step: 0,
                    };
                    // Return the color one more time before starting fade
                    color
                } else {
                    // Still in static phase
                    *elapsed_frames += 1;
                    *color
                }
            }
        }
    }
}

/// Two-layer (base + overlay) LED effect renderer. The host frame loop calls
/// [`Self::poll_messages`] and [`Self::render`] once per frame and pushes the
/// resulting buffer to the physical (or virtual) LEDs.
pub struct LedProcessor {
    base_layer: [LedEffect; NUM_LEDS],
    overlay_layer: [LedEffect; NUM_LEDS],
    buffer: [RGB8; NUM_LEDS],
}

impl Default for LedProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl LedProcessor {
    pub fn new() -> Self {
        Self {
            base_layer: [LedEffect::Off; NUM_LEDS],
            overlay_layer: [LedEffect::Off; NUM_LEDS],
            buffer: [BLACK; NUM_LEDS],
        }
    }

    /// Sets a base-layer effect directly, bypassing the per-LED signals.
    pub fn set_base_mode(&mut self, no: usize, mode: LedMode) {
        self.base_layer[no] = mode.into_effect();
    }

    /// Drains pending per-LED signals and overlay messages into the layers.
    pub fn poll_messages(&mut self) {
        for (i, led_signal) in LED_SIGNALS.iter().enumerate() {
            if let Some(msg) = led_signal.try_take() {
                match msg {
                    LedMsg::Set(mode) => {
                        self.base_layer[i] = mode.into_effect();
                    }
                    LedMsg::Reset => match self.base_layer[i] {
                        LedEffect::Static { color, brightness } => {
                            self.base_layer[i] = LedEffect::FadeOut {
                                from: color.scale(brightness),
                                step: 0,
                            }
                        }
                        LedEffect::StaticFade { color, .. } => {
                            self.base_layer[i] = LedEffect::FadeOut {
                                from: color,
                                step: 0,
                            }
                        }
                        _ => {
                            self.base_layer[i] = LedEffect::Off;
                        }
                    },
                }
            }
        }

        while let Ok((no, mode)) = LED_OVERLAY_CHANNEL.try_receive() {
            self.overlay_layer[no] = match mode {
                Some(m) => m.into_effect(),
                None => LedEffect::Off,
            };
        }
    }

    /// Advances all effects by one frame and returns the rendered colors.
    pub fn render(&mut self) -> &[RGB8; NUM_LEDS] {
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
        &self.buffer
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
    LED_OVERLAY_CHANNEL.send((no, Some(mode))).await;
}

pub async fn clear_led_overlay(channel: usize, position: Led) {
    let no = get_no(channel, position);
    LED_OVERLAY_CHANNEL.send((no, None)).await;
}
