use embassy_futures::{
    join::{join, join3, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    quantizer::Pitch,
    utils::attenuate_bipolar,
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Note, Param,
    Range, Value, VoltPerOct, APP_MAX_PARAMS,
};

use crate::{
    app::{App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent},
    apps::coltrane_geo::{build_coltrane_chord, build_cycle, center_color},
    apps::follow_key,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 15;

const BUTTON_DUCK_MS: u16 = 25;
const SOUNDING_CAP: usize = 8;

const DIV_LABELS: &[&str] = &[
    "1/1", "1/2", "1/4", "1/8", "1/16", "1/32", "1/4t", "1/8t", "1/16t", "1/32t", "1/64t",
];
const RESOLUTION: [u32; 11] = [96, 48, 24, 12, 6, 3, 16, 8, 4, 2, 1];

const JACK_OUT: usize = 0;
const JACK_IN_DENSITY: usize = 1;
const JACK_IN_RESET: usize = 2;
const JACK_COUNT: usize = 3;
const TRIG_HIGH: u16 = 2458;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Giant Steps",
    "Clock-driven chord sequencer over Coltrane tonal center geometry",
    Color::Blue,
    AppIcon::NoteGrid,
)
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "Root" })
.add_param(Param::Enum {
    name: "Division",
    variants: DIV_LABELS,
})
.add_param(Param::Enum {
    name: "Interval",
    variants: &["Minor 3rd", "Major 3rd", "Perfect 4th", "Tritone"],
})
.add_param(Param::Enum {
    name: "Voicing",
    variants: &["Triad", "7th", "Drop-2", "Quartal"],
})
.add_param(Param::Enum {
    name: "Direction",
    variants: &["Forward", "Reverse", "Pendulum", "Random"],
})
.add_param(Param::i32 {
    name: "Velocity",
    min: 1,
    max: 127,
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Orange,
        Color::Cyan,
        Color::Rose,
        Color::Violet,
        Color::Pink,
        Color::Yellow,
    ],
})
.add_param(Param::Enum {
    name: "Jack",
    variants: &["CV Out", "CV In Density", "CV In Reset"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::VoltPerOct)
.add_param(Param::i32 {
    name: "CV Att",
    min: 0,
    max: 100,
})
.add_param(Param::bool {
    name: "Follow device tonic",
})
.add_param(Param::bool {
    name: "Follow device scale",
});

pub struct Params {
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    root: MidiNote,
    division: usize,
    interval: usize,
    voicing: usize,
    direction: usize,
    velocity: i32,
    color: Color,
    jack: usize,
    range: Range,
    vpo: VoltPerOct,
    cv_att: i32,
    follow_tonic: bool,
    follow_scale: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_out: MidiOut([true, false, false]),
            midi_channel: MidiChannel::default(),
            root: MidiNote::from(48),
            division: 3,
            interval: 1,
            voicing: 1,
            direction: 0,
            velocity: 100,
            color: Color::Blue,
            jack: JACK_OUT,
            range: Range::_0_10V,
            vpo: VoltPerOct::Standard,
            cv_att: 100,
            follow_tonic: true,
            follow_scale: false,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_out: MidiOut::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            root: MidiNote::from_value(values[2]),
            division: usize::from_value(values[3]).min(RESOLUTION.len() - 1),
            interval: usize::from_value(values[4]).min(3),
            voicing: usize::from_value(values[5]).min(3),
            direction: usize::from_value(values[6]).min(3),
            velocity: i32::from_value(values[7]).clamp(1, 127),
            color: Color::from_value(values[8]),
            jack: usize::from_value(values[9]).min(JACK_COUNT - 1),
            range: Range::from_value(values[10]),
            vpo: VoltPerOct::from_value(values[11]),
            cv_att: i32::from_value(values[12]).clamp(0, 100),
            follow_tonic: bool::from_value(values[13]),
            follow_scale: bool::from_value(values[14]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut v = Vec::new();
        v.push(self.midi_out.into()).unwrap();
        v.push(self.midi_channel.into()).unwrap();
        v.push(self.root.into()).unwrap();
        v.push(self.division.into()).unwrap();
        v.push(self.interval.into()).unwrap();
        v.push(self.voicing.into()).unwrap();
        v.push(self.direction.into()).unwrap();
        v.push(self.velocity.into()).unwrap();
        v.push(self.color.into()).unwrap();
        v.push(self.jack.into()).unwrap();
        v.push(self.range.into()).unwrap();
        v.push(self.vpo.into()).unwrap();
        v.push(self.cv_att.into()).unwrap();
        v.push(self.follow_tonic.into()).unwrap();
        v.push(self.follow_scale.into()).unwrap();
        v
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Storage {
    density_saved: u16,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            density_saved: 2048,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

fn att_from_pct(pct: i32) -> u16 {
    ((pct.clamp(0, 100) as u32 * 4095) / 100) as u16
}

fn interval_semitones(param: usize) -> u8 {
    match param {
        0 => 3,
        1 => 4,
        2 => 5,
        3 => 6,
        _ => 4,
    }
}

fn velocity_12bit(vel_7: i32) -> u16 {
    let v = vel_7.clamp(1, 127) as u32;
    ((v * 4095) / 127) as u16
}

fn density_from_fader(v: u16) -> u8 {
    if v < 1366 {
        0
    } else if v < 2731 {
        1
    } else {
        2
    }
}

fn note_to_pitch(note: u8) -> Pitch {
    let octave = (note as i16 / 12) - 1;
    let pc = note % 12;
    Pitch {
        octave: octave as i8,
        note: Note::from(pc),
        raw: None,
    }
}

#[embassy_executor::task(pool_size = 4)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id, Params::default());
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
    let (
        midi_out,
        midi_chan,
        root_note,
        division,
        interval_param,
        voicing,
        direction,
        velocity,
        led_color,
        jack_param,
        range,
        vpo,
        cv_att,
        follow_tonic,
        _follow_scale,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.root,
            p.division,
            p.interval,
            p.voicing,
            p.direction,
            p.velocity,
            p.color,
            p.jack.min(JACK_COUNT - 1),
            p.range,
            p.vpo,
            att_from_pct(p.cv_att),
            p.follow_tonic,
            p.follow_scale,
        )
    });

    let vel12 = velocity_12bit(velocity);
    let interval_semi = interval_semitones(interval_param);

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let die = app.use_die();
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let out_jack = if jack_param == JACK_OUT {
        Some(app.make_out_jack(0, range).await)
    } else {
        None
    };
    let in_jack = if jack_param != JACK_OUT {
        Some(app.make_in_jack(0, Range::_Neg5_5V).await)
    } else {
        None
    };

    let (density0, muted0) = storage.query(|s| (s.density_saved, s.muted));

    let glob_density = app.make_global(density0);
    let glob_div = app.make_global(RESOLUTION[division.min(RESOLUTION.len() - 1)]);
    let glob_muted = app.make_global(muted0);
    let glob_latch = app.make_global(LatchLayer::Main);
    let glob_fader_moved = app.make_global(false);
    let glob_button_duck = app.make_global(0u16);
    let glob_center_idx = app.make_global(0u8);
    let glob_reset = app.make_global(false);
    let glob_reverse = app.make_global(false);
    let glob_cv_val = app.make_global(2047u16);
    let glob_fader_dirty = app.make_global(false);
    let long_press_fired = app.make_global(false);

    if muted0 {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let fut_cv = async {
        let mut prev_gate_high = false;
        loop {
            app.delay_millis(2).await;
            if let Some(ref input) = in_jack {
                let raw = input.get_value();
                let in_val = attenuate_bipolar(raw, cv_att);
                glob_cv_val.set(in_val);
                if jack_param == JACK_IN_RESET {
                    let high = in_val >= TRIG_HIGH;
                    if high && !prev_gate_high {
                        glob_reset.set(true);
                    }
                    prev_gate_high = high;
                } else {
                    prev_gate_high = false;
                }
            }
        }
    };

    let fut_engine = async {
        let mut sounding: Vec<u8, SOUNDING_CAP> = Vec::new();
        let mut step: usize = 0;
        let mut pendulum_fwd = true;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Stop => {
                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();
                }
                ClockEvent::Start | ClockEvent::Reset => {
                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();
                    step = 0;
                    pendulum_fwd = true;
                }
                ClockEvent::Tick(t) => {
                    let div = glob_div.get().max(1) as u64;
                    if t == 0 || !t.is_multiple_of(div) {
                        continue;
                    }

                    if glob_muted.get() {
                        continue;
                    }

                    if glob_reset.get() {
                        glob_reset.set(false);
                        step = 0;
                        pendulum_fwd = true;
                    }

                    let density_fader = if jack_param == JACK_IN_DENSITY {
                        let base = glob_density.get();
                        (base as i32 + glob_cv_val.get() as i32 - 2047).clamp(0, 4095) as u16
                    } else {
                        glob_density.get()
                    };
                    let density = density_from_fader(density_fader);

                    let root_midi = follow_key::root(follow_tonic, root_note);
                    let cycle = build_cycle(interval_semi, density);
                    let cycle_len = cycle.len();

                    if cycle_len == 0 {
                        continue;
                    }

                    let idx = step % cycle_len;
                    let cs = cycle[idx];

                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();

                    let chord_root = (root_midi as u16 + cs.root_offset as u16).min(127) as u8;
                    let notes = build_coltrane_chord(chord_root, cs.quality, voicing);
                    for &n in notes.iter() {
                        midi.send_note_on(MidiNote::from(n), vel12).await;
                        let _ = sounding.push(n);
                    }

                    if let Some(ref jack) = out_jack {
                        jack.set_value(note_to_pitch(chord_root).as_counts(range, vpo));
                    }

                    glob_center_idx.set(cs.center);
                    glob_button_duck.set(BUTTON_DUCK_MS);

                    let reversed = glob_reverse.get();
                    let dir = if reversed {
                        match direction {
                            0 => 1,
                            1 => 0,
                            d => d,
                        }
                    } else {
                        direction
                    };
                    match dir {
                        1 => {
                            if step == 0 {
                                step = cycle_len - 1;
                            } else {
                                step -= 1;
                            }
                        }
                        2 => {
                            if pendulum_fwd {
                                step += 1;
                                if step >= cycle_len {
                                    step = cycle_len.saturating_sub(2);
                                    pendulum_fwd = false;
                                }
                            } else if step == 0 {
                                step = 1.min(cycle_len - 1);
                                pendulum_fwd = true;
                            } else {
                                step -= 1;
                            }
                        }
                        3 => {
                            step = (die.roll() as usize) % cycle_len;
                        }
                        _ => {
                            step = (step + 1) % cycle_len;
                        }
                    }
                }
            }
        }
    };

    let fut_faders = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change_at(0).await;
            let layer = glob_latch.get();
            glob_fader_moved.set(true);

            let target = match layer {
                LatchLayer::Main => glob_density.get(),
                _ => glob_density.get(),
            };

            if let Some(v) = latch.update(faders.get_value(), layer, target) {
                if layer == LatchLayer::Main {
                    glob_density.set(v);
                }
                glob_fader_dirty.set(true);
            }
        }
    };

    let fut_buttons = async {
        loop {
            let (_, _down_shift) = buttons.wait_for_any_down().await;
            long_press_fired.set(false);
            glob_fader_moved.set(false);
            buttons.wait_for_up(0).await;

            if long_press_fired.get() {
                continue;
            }
            if !glob_fader_moved.get() {
                glob_reset.set(true);
            }
        }
    };

    let fut_long = async {
        loop {
            let (_, _shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            let rev = !glob_reverse.get();
            glob_reverse.set(rev);
        }
    };

    let fut_persist = async {
        loop {
            app.delay_millis(400).await;
            if !glob_fader_dirty.get() {
                continue;
            }
            glob_fader_dirty.set(false);
            storage.modify_and_save(|st| {
                st.density_saved = glob_density.get();
                st.muted = glob_muted.get();
            });
        }
    };

    let fut_leds = async {
        loop {
            app.delay_millis(8).await;

            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else {
                LatchLayer::Main
            };
            glob_latch.set(layer);

            let duck_active = {
                let d = glob_button_duck.get();
                if d > 0 {
                    glob_button_duck.set(d.saturating_sub(8));
                    true
                } else {
                    false
                }
            };

            let muted = glob_muted.get();
            let center = glob_center_idx.get();
            let (cr, cg, cb) = center_color(center);
            let center_col = Color::Custom(cr, cg, cb);

            if !muted {
                leds.set(0, Led::Top, center_col, Brightness::Mid);
                leds.set(0, Led::Bottom, center_col, Brightness::Low);
                let btn_bright = if duck_active {
                    Brightness::Low
                } else {
                    Brightness::Mid
                };
                leds.set(0, Led::Button, center_col, btn_bright);
            } else {
                leds.unset(0, Led::Top);
                leds.unset(0, Led::Bottom);
                leds.unset(0, Led::Button);
            }
        }
    };

    let fut_scene = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(_) => {
                    let (d, m) = storage.query(|s| (s.density_saved, s.muted));
                    glob_density.set(d);
                    glob_muted.set(m);
                    let div = params.query(|p| p.division);
                    glob_div.set(RESOLUTION[div.min(RESOLUTION.len() - 1)]);
                }
                SceneEvent::SaveScene(_) => {}
            }
        }
    };

    join(
        join5(
            fut_engine,
            fut_cv,
            fut_faders,
            fut_leds,
            fut_scene,
        ),
        join3(fut_buttons, fut_long, fut_persist),
    )
    .await;
}
