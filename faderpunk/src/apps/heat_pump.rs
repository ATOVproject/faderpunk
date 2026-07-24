use embassy_futures::{
    join::{join, join4},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate_bipolar, midi_gate},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};

use crate::{
    app::{App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent},
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 9;

const MIN_RELEASE_MS: f32 = 10.0;
/// Tick spacing at 24 PPQN (whole-note / N), slow → fast.
const DIVISION_TICKS: [u32; 10] = [96, 48, 36, 24, 18, 16, 12, 8, 6, 3];
/// Invert gesture LED feedback length (white↔off fade).
const INVERT_FADE_MS: u16 = 500;
/// Max button brightness duck length — matches global BPM metronome (`METRONOME_HIGH_MS`).
/// Actual duck is capped to half the current pulse slot so Mid stays visible on fast divisions.
const BUTTON_DUCK_MS: u16 = 25;
const BUTTON_DUCK_MIN_MS: u16 = 4;
/// Hold off periodic button LED writes so LedMode::Flash can finish.
const BUTTON_FLASH_MS: u16 = 850;

const JACK_OUT: usize = 0;
const JACK_IN: usize = 1;

const DEST_DEPTH: usize = 0;
const DEST_RELEASE: usize = 1;
const DEST_DUCK: usize = 2;
const DEST_COUNT: usize = 3;

/// Rising-edge threshold for trigger dest (bipolar mid + margin), same as Super LFO Reset.
const TRIG_HIGH: u16 = 2458;

/// Base palette for division identity. Index 0 is White; for a user-chosen
/// color we rotate so that color lands on 1/1 (see `color_for_division`).
const DIVISION_COLORS: [Color; 10] = [
    Color::White,
    Color::Yellow,
    Color::Orange,
    Color::Red,
    Color::Lime,
    Color::Green,
    Color::Cyan,
    Color::SkyBlue,
    Color::Blue,
    Color::Violet,
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "Heat Pump",
    "Clock-synced sidechain ducking envelope",
    Color::Pink,
    AppIcon::AdEnv,
)
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Pink,
        Color::Rose,
        Color::Orange,
        Color::Yellow,
        Color::Cyan,
        Color::Blue,
        Color::Violet,
        Color::Green,
    ],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiNrpn)
.add_param(Param::MidiOut)
.add_param(Param::Enum {
    name: "Jack",
    variants: &["CV Out", "CV In"],
})
.add_param(Param::Enum {
    name: "CV Dest",
    variants: &["Depth", "Release", "Duck"],
})
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 100,
});

pub struct Params {
    color: Color,
    range: Range,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    nrpn: bool,
    midi_out: MidiOut,
    jack_mode: usize,
    dest: usize,
    cv_att: i32,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Legacy layout had Division at [0]; shift indices if present.
        let (color, range, midi_channel, midi_cc, nrpn, midi_out, jack_mode, dest, cv_att) =
            if values.len() >= 9 {
                (
                    Color::from_value(values[0]),
                    Range::from_value(values[1]),
                    MidiChannel::from_value(values[2]),
                    MidiCc::from_value(values[3]),
                    bool::from_value(values[4]),
                    MidiOut::from_value(values[5]),
                    usize::from_value(values[6]),
                    usize::from_value(values[7]),
                    i32::from_value(values[8]),
                )
            } else if values.len() >= 7 {
                (
                    Color::from_value(values[1]),
                    Range::from_value(values[2]),
                    MidiChannel::from_value(values[3]),
                    MidiCc::from_value(values[4]),
                    bool::from_value(values[5]),
                    MidiOut::from_value(values[6]),
                    JACK_OUT,
                    DEST_DEPTH,
                    100,
                )
            } else if values.len() >= 6 {
                (
                    Color::from_value(values[0]),
                    Range::from_value(values[1]),
                    MidiChannel::from_value(values[2]),
                    MidiCc::from_value(values[3]),
                    bool::from_value(values[4]),
                    MidiOut::from_value(values[5]),
                    JACK_OUT,
                    DEST_DEPTH,
                    100,
                )
            } else {
                return None;
            };
        Some(Self {
            color,
            range,
            midi_channel,
            midi_cc,
            nrpn,
            midi_out,
            jack_mode: jack_mode.min(1),
            dest: dest.min(DEST_COUNT - 1),
            cv_att: cv_att.clamp(0, 100),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.jack_mode.into()).unwrap();
        vec.push(self.dest.into()).unwrap();
        vec.push(self.cv_att.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    release: u16,
    depth: u16,
    invert: bool,
    muted: bool,
    /// Live division index; always starts at 1/1 (0) for new instances.
    #[serde(default = "default_division")]
    division: usize,
    #[serde(default)]
    dest: usize,
    #[serde(default = "default_in_att")]
    in_att: u16,
}

fn default_division() -> usize {
    0 // 1/1 — user color
}

fn default_in_att() -> u16 {
    4095
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            release: 2000,
            depth: 2800,
            invert: false,
            muted: false,
            division: default_division(),
            dest: DEST_DEPTH,
            in_att: default_in_att(),
        }
    }
}

impl AppStorage for Storage {}

fn idle_level(invert: bool) -> f32 {
    if invert {
        0.0
    } else {
        4095.0
    }
}

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 100) as u32 * 4095) / 100) as u16
}

fn mod_u16(base: u16, in_val: u16) -> u16 {
    (base as i32 + in_val as i32 - 2047).clamp(0, 4095) as u16
}

fn dest_color(dest: usize) -> Color {
    match dest {
        DEST_DEPTH => Color::Red,
        DEST_RELEASE => Color::Yellow,
        DEST_DUCK => Color::Cyan,
        _ => Color::Yellow,
    }
}

/// Color for a division: rotate `DIVISION_COLORS` so the user-set color sits
/// at 1/1. Rotation offset = distance of that color from White (index 0).
/// If the user color isn't in the palette, 1/1 uses it directly and the rest
/// keep the base palette from Yellow onward.
fn color_for_division(user: Color, division: usize) -> Color {
    let div = division.min(DIVISION_COLORS.len() - 1);
    if let Some(offset) = DIVISION_COLORS.iter().position(|&c| c == user) {
        DIVISION_COLORS[(offset + div) % DIVISION_COLORS.len()]
    } else if div == 0 {
        user
    } else {
        DIVISION_COLORS[div]
    }
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            color: Color::Pink,
            range: Range::_0_10V,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
            nrpn: false,
            midi_out: MidiOut([false, false, false]),
            jack_mode: JACK_OUT,
            dest: DEST_DEPTH,
            cv_att: 100,
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

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (led_color, range, jack_mode, p_dest, p_att) =
        params.query(|p| (p.color, p.range, p.jack_mode, p.dest, p.cv_att));
    let (midi_out, midi_chan, midi_cc, nrpn) =
        params.query(|p| (p.midi_out, p.midi_channel, p.midi_cc, p.nrpn));

    // Configurator Dest/Att are start values; apply on each run() (param edits restart run).
    storage.modify_and_save(|s| {
        s.dest = p_dest.min(DEST_COUNT - 1);
        s.in_att = att_from_pct(p_att);
    });

    let output = if jack_mode == JACK_OUT {
        Some(app.make_out_jack(0, range).await)
    } else {
        None
    };
    let input = if jack_mode == JACK_IN {
        Some(app.make_in_jack(0, Range::_Neg5_5V).await)
    } else {
        None
    };

    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let midi = app.use_midi_output(midi_out, midi_chan, nrpn);

    let (release, depth, invert, muted, division_idx, dest, in_att) = storage.query(|s| {
        (
            s.release,
            s.depth,
            s.invert,
            s.muted,
            s.division.min(DIVISION_TICKS.len() - 1),
            s.dest.min(DEST_COUNT - 1),
            s.in_att,
        )
    });

    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_release = app.make_global(release);
    let glob_depth = app.make_global(depth);
    let glob_invert = app.make_global(invert);
    let glob_muted = app.make_global(muted);
    let glob_division = app.make_global(division_idx);
    let glob_trigger = app.make_global(false);
    let glob_level = app.make_global(idle_level(invert));
    let glob_dest = app.make_global(dest);
    let glob_in_att = app.make_global(in_att);
    let long_press_fired = app.make_global(false);
    // Remaining ms of invert LED fade; 0 = inactive.
    let glob_invert_fade = app.make_global(0u16);
    // true = none→white, false = white→none.
    let glob_invert_fade_up = app.make_global(false);
    let glob_btn_flash = app.make_global(0u16);

    if muted {
        leds.unset(0, Led::Button);
    }

    let main_loop = async {
        let mut last_midi_val: u16 = u16::MAX;
        let mut button_duck_left: u16 = 0;
        let mut last_button_slot: u8 = 0xff;
        let mut ms_in_slot: u16 = 0;
        let mut last_blink_division: usize = usize::MAX;
        let mut prev_gate_high = false;
        loop {
            app.delay_millis(1).await;

            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let invert = glob_invert.get();
            let idle = idle_level(invert);
            let muted = glob_muted.get();

            let mut eff_depth = glob_depth.get();
            let mut eff_release = glob_release.get();

            if let Some(ref input) = input {
                let in_val = attenuate_bipolar(input.get_value(), glob_in_att.get());
                let destination = glob_dest.get().min(DEST_COUNT - 1);
                match destination {
                    DEST_DEPTH => {
                        eff_depth = mod_u16(eff_depth, in_val);
                        prev_gate_high = false;
                    }
                    DEST_RELEASE => {
                        eff_release = mod_u16(eff_release, in_val);
                        prev_gate_high = false;
                    }
                    DEST_DUCK => {
                        let high = in_val >= TRIG_HIGH;
                        if high && !prev_gate_high {
                            glob_trigger.set(true);
                        }
                        prev_gate_high = high;
                    }
                    _ => {
                        prev_gate_high = false;
                    }
                }
            }

            let mut level = glob_level.get();

            if glob_trigger.get() {
                glob_trigger.set(false);
                level = if invert {
                    eff_depth as f32
                } else {
                    4095u16.saturating_sub(eff_depth) as f32
                };
            }

            // Fader up = faster recovery: invert the fader before the curve.
            let release_ms =
                Curve::Exponential.at(4095u16.saturating_sub(eff_release)) as f32 + MIN_RELEASE_MS;
            let step = 4095.0 / release_ms;
            if level < idle {
                level = (level + step).min(idle);
            } else if level > idle {
                level = (level - step).max(idle);
            }
            glob_level.set(level);

            let out = level as u16;
            // Mute = bypass the pump: hold idle so CV/MIDI don't stick at silence
            // (e.g. CC7 volume frozen at 0 on a Minitaur).
            let effective_out = if muted { idle as u16 } else { out };
            if let Some(ref output) = output {
                output.set_value(effective_out);
            }

            if midi_out.is_some() {
                let gate_val = midi_gate(effective_out, nrpn);
                if gate_val != last_midi_val {
                    midi.send_cc(midi_cc, effective_out).await;
                    last_midi_val = gate_val;
                }
            }

            let flash_left = glob_btn_flash.get();
            if flash_left > 0 {
                glob_btn_flash.set(flash_left.saturating_sub(1));
            }

            // Invert feedback (white↔off) suppresses the division metronome.
            let fade_left = glob_invert_fade.get();
            if fade_left > 0 {
                let elapsed = INVERT_FADE_MS.saturating_sub(fade_left);
                let bright = if glob_invert_fade_up.get() {
                    ((elapsed as u32 * 255) / INVERT_FADE_MS as u32) as u8
                } else {
                    (((INVERT_FADE_MS - elapsed) as u32 * 255) / INVERT_FADE_MS as u32) as u8
                };
                leds.set(0, Led::Button, Color::White, Brightness::Custom(bright));
                glob_invert_fade.set(fade_left.saturating_sub(1));
            } else if flash_left == 0 {
                // Button metronome: Mid→Low duck up to 4× per division.
                // Duck length tracks slot duration (≤25ms, half Mid) so fast
                // divisions still blink instead of sticking on Low.
                let division = glob_division.get().min(DIVISION_TICKS.len() - 1);
                let div_ticks = DIVISION_TICKS[division];
                let div_color = color_for_division(led_color, division);
                if muted {
                    button_duck_left = 0;
                    last_button_slot = 0xff;
                    ms_in_slot = 0;
                    leds.unset(0, Led::Button);
                } else if latch_active_layer == LatchLayer::Alt && jack_mode == JACK_IN {
                    leds.set(
                        0,
                        Led::Button,
                        dest_color(glob_dest.get()),
                        Brightness::Mid,
                    );
                } else {
                    if division != last_blink_division {
                        last_blink_division = division;
                        last_button_slot = 0xff;
                        button_duck_left = 0;
                        ms_in_slot = 0;
                    }
                    let phase = (ticks() as u32) % div_ticks;
                    let slot = (phase * 4 / div_ticks) as u8;
                    if slot != last_button_slot {
                        if last_button_slot != 0xff {
                            let duck = (ms_in_slot / 2).min(BUTTON_DUCK_MS);
                            button_duck_left = if duck >= BUTTON_DUCK_MIN_MS { duck } else { 0 };
                        }
                        last_button_slot = slot;
                        ms_in_slot = 0;
                    }
                    ms_in_slot = ms_in_slot.saturating_add(1);

                    let brightness = if button_duck_left > 0 {
                        button_duck_left -= 1;
                        Brightness::Low
                    } else {
                        Brightness::Mid
                    };
                    leds.set(0, Led::Button, div_color, brightness);
                }
            }

            match latch_active_layer {
                LatchLayer::Main => {
                    leds.set(0, Led::Top, led_color, Brightness::Custom((out / 16) as u8));
                    leds.unset(0, Led::Bottom);
                }
                LatchLayer::Alt => {
                    let depth = glob_depth.get();
                    leds.set(
                        0,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom((depth / 16) as u8),
                    );
                    leds.unset(0, Led::Bottom);
                }
                LatchLayer::Third => {
                    if jack_mode == JACK_IN {
                        let att = glob_in_att.get();
                        leds.set(
                            0,
                            Led::Top,
                            Color::Cyan,
                            Brightness::Custom((att / 16) as u8),
                        );
                    }
                    leds.unset(0, Led::Bottom);
                }
            }
        }
    };

    let clock_handler = async {
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick => {
                    let div_ticks =
                        DIVISION_TICKS[glob_division.get().min(DIVISION_TICKS.len() - 1)];
                    let clkn = ticks() as u32;
                    if clkn.is_multiple_of(div_ticks) {
                        glob_trigger.set(true);
                    }
                }
                ClockEvent::Reset | ClockEvent::Stop | ClockEvent::Start => {}
            }
        }
    };

    let fader_handler = async {
        let mut latch = app.make_latch(faders.get_value());

        loop {
            faders.wait_for_change().await;
            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.release),
                LatchLayer::Alt => storage.query(|s| s.depth),
                LatchLayer::Third => {
                    if jack_mode == JACK_IN {
                        storage.query(|s| s.in_att)
                    } else {
                        0
                    }
                }
            };

            if let Some(new_value) = latch.update(faders.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_release.set(new_value);
                        storage.modify_and_save(|s| {
                            s.release = new_value;
                        });
                    }
                    LatchLayer::Alt => {
                        glob_depth.set(new_value);
                        storage.modify_and_save(|s| {
                            s.depth = new_value;
                        });
                    }
                    LatchLayer::Third => {
                        if jack_mode == JACK_IN {
                            glob_in_att.set(new_value);
                            storage.modify_and_save(|s| {
                                s.in_att = new_value;
                            });
                        }
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            buttons.wait_for_down(0).await;

            if buttons.is_shift_pressed() {
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    if jack_mode == JACK_IN {
                        // Shift + short (CV In): cycle destination + flash dest color.
                        let next = storage.modify_and_save(|s| {
                            s.dest = (s.dest + 1) % DEST_COUNT;
                            s.dest
                        });
                        glob_dest.set(next);
                        leds.set_mode(0, Led::Button, LedMode::Flash(dest_color(next), Some(3)));
                        glob_btn_flash.set(BUTTON_FLASH_MS);
                    } else {
                        // Shift + short (CV Out): cycle division (1/1 = user color).
                        let next = (glob_division.get() + 1) % DIVISION_TICKS.len();
                        glob_division.set(next);
                        storage.modify_and_save(|s| {
                            s.division = next;
                        });
                    }
                }
            } else {
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    glob_trigger.set(true);
                }
            }
        }
    };

    let long_press = async {
        loop {
            let (_, is_shift_pressed) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            if is_shift_pressed {
                // Shift + long: invert duck direction
                let invert = storage.modify_and_save(|s| {
                    s.invert = !s.invert;
                    s.invert
                });
                glob_invert.set(invert);
                glob_level.set(idle_level(invert));
                // Invert on → white→none; invert off → none→white.
                glob_invert_fade_up.set(!invert);
                glob_invert_fade.set(INVERT_FADE_MS);
            } else {
                let muted = glob_muted.toggle();
                storage.modify_and_save(|s| {
                    s.muted = muted;
                });
                if muted {
                    leds.unset(0, Led::Button);
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (release, depth, invert, muted, division, dest, in_att) =
                        storage.query(|s| {
                            (
                                s.release,
                                s.depth,
                                s.invert,
                                s.muted,
                                s.division.min(DIVISION_TICKS.len() - 1),
                                s.dest.min(DEST_COUNT - 1),
                                s.in_att,
                            )
                        });
                    glob_release.set(release);
                    glob_depth.set(depth);
                    glob_invert.set(invert);
                    glob_muted.set(muted);
                    glob_division.set(division);
                    glob_dest.set(dest);
                    glob_in_att.set(in_att);
                    glob_level.set(idle_level(invert));
                    if muted {
                        leds.unset(0, Led::Button);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join(
        scene_handler,
        join4(
            main_loop,
            clock_handler,
            fader_handler,
            join(button_handler, long_press),
        ),
    )
    .await;
}
