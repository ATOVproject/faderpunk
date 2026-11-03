use embassy_futures::{
    join::join4,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{clickless, slew_lin, SlewState},
    AppIcon, Brightness, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, OutJack, ParamStore, SceneEvent};

/// Channel 0 is the sidechain input, the remaining channels are mixer outputs.
/// This is the only number to change for a variant with more (or fewer)
/// control channels — it must stay a bare integer literal (the simulator
/// catalog generator reads it), so everything else derives from it.
pub const CHANNELS: usize = 3;
/// Number of control (output) channels.
const CTRL: usize = CHANNELS - 1;
pub const PARAMS: usize = 3;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Mid;

/// Input mode param values.
const MODE_TRIGGER: usize = 0;

/// Rising-edge threshold: +1V on the bipolar (-5..5V) input range.
const TRIG_HIGH: u16 = 2458;
/// Re-arm threshold (~+0.5V), gives the edge detector some hysteresis.
const TRIG_LOW: u16 = 2252;

const MIN_DECAY_MS: f32 = 10.0;
/// Hold at the ducked floor as a fraction of the measured trigger period.
const HOLD_PERIOD_NUM: u32 = 1;
const HOLD_PERIOD_DEN: u32 = 8;
/// Inferred trigger period clamp (ms) — seeds hold length across tempos.
const PERIOD_MS_MIN: u32 = 40;
const PERIOD_MS_MAX: u32 = 8000;
const PERIOD_MS_DEFAULT: u32 = 500;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Sidechain Mixer",
    "CV mixer ducked by a trigger or audio sidechain",
    Color::Cyan,
    AppIcon::EnvFollower,
)
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Cyan,
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::Color {
    name: "Input color",
    variants: &[
        Color::Pink,
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::Enum {
    name: "Input mode",
    variants: &["Trigger", "Audio"],
});

pub struct Params {
    color: Color,
    input_color: Color,
    input_mode: usize,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            color: Color::from_value(values[0]),
            input_color: Color::from_value(values[1]),
            input_mode: usize::from_value(values[2]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.color.into()).unwrap();
        vec.push(self.input_color.into()).unwrap();
        vec.push(self.input_mode.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Attack fader (Shift + fader 0). Higher = slower.
    attack: u16,
    /// Decay fader (fader 0). Higher = slower.
    decay: u16,
    /// Audio input gain (input button + fader 0), audio mode only. 0 = 1x, 4095 = 3x.
    audio_gain: u16,
    /// Channel level fader (fader n).
    level: [u16; CTRL],
    /// Maximum output voltage (Shift + fader n).
    max_v: [u16; CTRL],
    /// Sidechain depth (input button + fader n).
    depth: [u16; CTRL],
    muted: [bool; CTRL],
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            attack: 0,
            decay: 2000,
            audio_gain: 0,
            level: [4095; CTRL],
            max_v: [4095; CTRL],
            depth: [2800; CTRL],
            muted: [false; CTRL],
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            color: Color::Cyan,
            input_color: Color::Pink,
            input_mode: MODE_TRIGGER,
        },
    );
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

    let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Attack,
    Hold,
    Decay,
}

/// Trigger-driven duck envelope: attack to the floor, hold, then log-shaped
/// recovery (same shape as the Heat Pump app). Output is the duck amount,
/// 0 (idle) ..= 4095 (fully ducked).
struct GateEnvelope {
    phase: Phase,
    level: f32,
    hold_left: u32,
    decay_elapsed: u32,
    ms_since_trig: u32,
    period_ms: u32,
}

impl GateEnvelope {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            level: 0.0,
            hold_left: 0,
            decay_elapsed: 0,
            ms_since_trig: 0,
            period_ms: PERIOD_MS_DEFAULT,
        }
    }

    /// Advance by 1 ms.
    fn step(&mut self, trigger: bool, attack: u16, decay: u16) -> u16 {
        self.ms_since_trig = self.ms_since_trig.saturating_add(1);

        if trigger {
            if self.ms_since_trig >= PERIOD_MS_MIN {
                self.period_ms = self.ms_since_trig.min(PERIOD_MS_MAX);
            }
            self.ms_since_trig = 0;
            // Cap hold at half the period so the decay still has room to breathe.
            self.hold_left = (self.period_ms * HOLD_PERIOD_NUM / HOLD_PERIOD_DEN)
                .clamp(1, (self.period_ms / 2).max(1));
            self.phase = Phase::Attack;
        }

        match self.phase {
            Phase::Idle => {}
            Phase::Attack => {
                // Attack starts from the current level so retriggers are click-free.
                let attack_ms = Curve::Exponential.at(attack) as f32 / 4.0;
                if attack_ms <= 1.0 {
                    self.level = 1.0;
                } else {
                    self.level = (self.level + 1.0 / attack_ms).min(1.0);
                }
                if self.level >= 1.0 {
                    self.phase = Phase::Hold;
                }
            }
            Phase::Hold => {
                self.hold_left = self.hold_left.saturating_sub(1);
                if self.hold_left == 0 {
                    self.decay_elapsed = 0;
                    self.phase = Phase::Decay;
                }
            }
            Phase::Decay => {
                let decay_ms = Curve::Exponential.at(decay) as f32 + MIN_DECAY_MS;
                self.decay_elapsed = self.decay_elapsed.saturating_add(1);
                let t = (self.decay_elapsed as f32 / decay_ms).min(1.0);
                // Log shape: leave the floor quickly, soft-land on idle.
                let shaped = Curve::Logarithmic.at((t * 4095.0) as u16) as f32 / 4095.0;
                self.level = 1.0 - shaped;
                if t >= 1.0 {
                    self.level = 0.0;
                    self.phase = Phase::Idle;
                }
            }
        }

        (self.level * 4095.0) as u16
    }
}

/// Bipolar input -> absolute amplitude, scaled to the full 12-bit range.
fn rectify(value: u16) -> u16 {
    (value.abs_diff(2047) as u32 * 2).min(4095) as u16
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (led_color, input_color, input_mode) =
        params.query(|p| (p.color, p.input_color, p.input_mode));

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    // Audio is bipolar; the same range works for gates (0V reads as mid-scale).
    let input = app.make_in_jack(0, Range::_Neg5_5V).await;
    let mut outputs: Vec<OutJack, CTRL> = Vec::new();
    for i in 0..CTRL {
        // Cannot fail: exactly CTRL jacks are pushed into a CTRL-sized Vec.
        let _ = outputs.push(app.make_out_jack(i + 1, Range::_0_10V).await);
    }

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let main_loop = async {
        let mut gate_env = GateEnvelope::new();
        let mut gate_high = false;
        let mut audio_env = SlewState::new();
        let mut base_smooth = [0u16; CTRL];
        let mut led_tick: u8 = 0;

        loop {
            app.delay_millis(1).await;

            let latch_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_layer);

            let (attack, decay) = storage.query(|s| (s.attack, s.decay));

            // Sidechain envelope, 0 (idle) ..= 4095 (fully ducked).
            let inval = input.get_value();
            let env = if input_mode == MODE_TRIGGER {
                let mut trigger = false;
                if !gate_high && inval >= TRIG_HIGH {
                    gate_high = true;
                    trigger = true;
                } else if gate_high && inval < TRIG_LOW {
                    gate_high = false;
                }
                gate_env.step(trigger, attack, decay)
            } else {
                let gain = storage.query(|s| s.audio_gain);
                // 1x..3x gain, curved like the Envelope Follower app's input gain.
                let boosted = (rectify(inval) as f32
                    * (Curve::Exponential.at(gain) as f32 * 2. / 4095. + 1.))
                    .clamp(0., 4095.) as u16;
                audio_env = slew_lin(audio_env, boosted, attack, decay);
                audio_env.value()
            };

            let (level, max_v, depth, muted) =
                storage.query(|s| (s.level, s.max_v, s.depth, s.muted));

            let mut out_vals = [0u16; CTRL];
            for i in 0..CTRL {
                let base = (level[i] as u32 * max_v[i] as u32 / 4095) as u16;
                let target = if muted[i] { 0 } else { base };
                // Smooth fader / mute steps only; the duck stays fast.
                base_smooth[i] = clickless(base_smooth[i], target);
                let duck = env as u32 * depth[i] as u32 / 4095;
                let out = (base_smooth[i] as u32 * (4095 - duck) / 4095) as u16;
                out_vals[i] = out;
                outputs[i].set_value(out);
            }

            led_tick = led_tick.wrapping_add(1);
            if !led_tick.is_multiple_of(4) {
                continue;
            }

            // Input channel
            leds.set(0, Led::Button, input_color, BUTTON_BRIGHTNESS);
            leds.unset(0, Led::Bottom);
            match latch_layer {
                LatchLayer::Alt => leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((attack / 16) as u8),
                ),
                LatchLayer::Third => {
                    let gain = storage.query(|s| s.audio_gain);
                    leds.set(0, Led::Top, Color::Red, Brightness::Custom((gain / 16) as u8));
                }
                LatchLayer::Main => leds.set(
                    0,
                    Led::Top,
                    input_color,
                    Brightness::Custom((env / 16) as u8),
                ),
            }

            // Control channels
            for i in 0..CTRL {
                let chan = i + 1;
                if muted[i] {
                    leds.unset(chan, Led::Button);
                } else {
                    leds.set(chan, Led::Button, led_color, BUTTON_BRIGHTNESS);
                }
                leds.unset(chan, Led::Bottom);
                match latch_layer {
                    LatchLayer::Main => leds.set(
                        chan,
                        Led::Top,
                        led_color,
                        Brightness::Custom((out_vals[i] / 16) as u8),
                    ),
                    LatchLayer::Alt => leds.set(
                        chan,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom((max_v[i] / 16) as u8),
                    ),
                    LatchLayer::Third => leds.set(
                        chan,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom((depth[i] / 16) as u8),
                    ),
                }
            }
        }
    };

    let fader_handler = async {
        let mut latches =
            core::array::from_fn::<_, CHANNELS, _>(|i| app.make_latch(faders.get_value_at(i)));

        loop {
            let chan = faders.wait_for_any_change().await;
            let layer = glob_latch_layer.get();
            let value = faders.get_value_at(chan);

            if chan == 0 {
                let target = match layer {
                    LatchLayer::Main => storage.query(|s| s.decay),
                    LatchLayer::Alt => storage.query(|s| s.attack),
                    LatchLayer::Third => storage.query(|s| s.audio_gain),
                };
                if let Some(new_value) = latches[0].update(value, layer, target) {
                    match layer {
                        LatchLayer::Main => storage.modify_and_save(|s| s.decay = new_value),
                        LatchLayer::Alt => storage.modify_and_save(|s| s.attack = new_value),
                        LatchLayer::Third => storage.modify_and_save(|s| s.audio_gain = new_value),
                    }
                }
            } else {
                let i = chan - 1;
                let target = match layer {
                    LatchLayer::Main => storage.query(|s| s.level[i]),
                    LatchLayer::Alt => storage.query(|s| s.max_v[i]),
                    LatchLayer::Third => storage.query(|s| s.depth[i]),
                };
                if let Some(new_value) = latches[chan].update(value, layer, target) {
                    match layer {
                        LatchLayer::Main => storage.modify_and_save(|s| s.level[i] = new_value),
                        LatchLayer::Alt => storage.modify_and_save(|s| s.max_v[i] = new_value),
                        LatchLayer::Third => storage.modify_and_save(|s| s.depth[i] = new_value),
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            // The input button is only a layer modifier; Shift + button is ignored
            // so a Shift gesture never toggles a mute by accident.
            if chan == 0 || is_shift_pressed {
                continue;
            }
            let i = chan - 1;
            storage.modify_and_save(|s| s.muted[i] = !s.muted[i]);
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => storage.load_from_scene(scene).await,
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join4(main_loop, fader_handler, button_handler, scene_handler).await;
}
