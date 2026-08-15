//! Axis Matrix -- playable Coltrane tonal center navigator.
//!
//! A 3x3 grid of tonal centers x harmonic functions. Main fader selects center
//! position (with snap zones and approach-chord transition zones at boundaries),
//! Alt fader selects harmonic function (I / V / ii). Clock triggers the chord
//! at the current grid position. Button tap toggles mute, hold enters drone,
//! Shift+tap latches, Shift+hold cycles the interval.

use embassy_futures::{
    join::{join3, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    quantizer::Pitch,
    AppIcon, Brightness, Color, Config, MidiChannel, MidiNote, MidiOut, Note, Param, Range, Value,
    VoltPerOct, APP_MAX_PARAMS,
};

use crate::{
    app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent},
    apps::coltrane_geo::{
        arp_order, build_chord_voice_led, center_brightness, center_from_app, feel_swing_ticks,
        feel_velocity, interval_color, step_div_mult, tritone_sub_root, ChordQuality, Motion,
        MOTION_LABELS,
    },
    apps::follow_key,
    tasks::global_config::get_global_config,
    tasks::leds::LedMode,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 15;

const BUTTON_DUCK_MS: u16 = 25;
const SOUNDING_CAP: usize = 8;
/// Hold off the periodic button paint so LedMode::Flash can finish.
const BUTTON_FLASH_MS: u16 = 550;
/// LED loop period; every ms countdown steps by this.
const LED_STEP_MS: u16 = 8;

/// CV out parking value while muted: 0 V, which is mid-scale on a bipolar range.
fn cv_idle(range: Range) -> u16 {
    if range.is_bipolar() {
        2048
    } else {
        0
    }
}

const DIV_LABELS: &[&str] = &[
    "1/1", "1/2", "1/4", "1/8", "1/16", "1/32", "1/4t", "1/8t", "1/16t", "1/32t", "1/64t",
];
const RESOLUTION: [u32; 11] = [96, 48, 24, 12, 6, 3, 16, 8, 4, 2, 1];

const JACK_OUT: usize = 0;
const JACK_IN_POS: usize = 1;
const JACK_IN_FUNC: usize = 2;
const JACK_COUNT: usize = 3;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Axis Matrix",
    "Playable Coltrane tonal center navigator - fader position, clock trigger",
    Color::Green,
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
    name: "Sustain",
    variants: &["Clock-gated", "Legato", "Staccato"],
})
.add_param(Param::i32 {
    name: "Velocity",
    min: 1,
    max: 127,
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Green,
        Color::Blue,
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
    variants: &["CV Out", "CV In Position", "CV In Function"],
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::VoltPerOct)
.add_param(Param::bool {
    name: "Follow device tonic",
})
.add_param(Param::bool {
    name: "Follow device scale",
})
.add_param(Param::Enum {
    name: "Motion",
    variants: MOTION_LABELS,
});

pub struct Params {
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    root: MidiNote,
    division: usize,
    interval: usize,
    voicing: usize,
    sustain: usize,
    velocity: i32,
    color: Color,
    jack: usize,
    range: Range,
    vpo: VoltPerOct,
    follow_tonic: bool,
    follow_scale: bool,
    motion: usize,
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
            sustain: 0,
            velocity: 100,
            color: Color::Green,
            jack: JACK_OUT,
            range: Range::_0_10V,
            vpo: VoltPerOct::Standard,
            follow_tonic: true,
            follow_scale: false,
            motion: 0,
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
            sustain: usize::from_value(values[6]).min(2),
            velocity: i32::from_value(values[7]).clamp(1, 127),
            color: Color::from_value(values[8]),
            jack: usize::from_value(values[9]).min(JACK_COUNT - 1),
            range: Range::from_value(values[10]),
            vpo: VoltPerOct::from_value(values[11]),
            follow_tonic: bool::from_value(values[12]),
            follow_scale: bool::from_value(values[13]),
            motion: usize::from_value(values[14]).min(3),
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
        v.push(self.sustain.into()).unwrap();
        v.push(self.velocity.into()).unwrap();
        v.push(self.color.into()).unwrap();
        v.push(self.jack.into()).unwrap();
        v.push(self.range.into()).unwrap();
        v.push(self.vpo.into()).unwrap();
        v.push(self.follow_tonic.into()).unwrap();
        v.push(self.follow_scale.into()).unwrap();
        v.push(self.motion.into()).unwrap();
        v
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Storage {
    position_saved: u16,
    function_saved: u16,
    muted: bool,
    interval_idx: u8,
    feel: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            position_saved: 2048,
            function_saved: 2048,
            muted: false,
            interval_idx: 1,
            feel: 0,
        }
    }
}

impl AppStorage for Storage {}

fn interval_semitones(param: usize) -> u8 {
    match param {
        0 => 3,
        1 => 4,
        2 => 5,
        3 => 6,
        _ => 4,
    }
}

/// Staccato gate: half the note's own window, never shorter than one tick.
fn gate_ticks(window: u64) -> u64 {
    (window / 2).max(1)
}

fn velocity_12bit(vel_7: i32) -> u16 {
    let v = vel_7.clamp(1, 127) as u32;
    ((v * 4095) / 127) as u16
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

fn position_to_center_and_approach(fader: u16) -> (u8, bool) {
    let zone = (fader / 1366).min(2) as u8;
    let within = fader % 1366;
    let in_transition = !(200..=1166).contains(&within);
    (zone, in_transition)
}

fn function_from_fader(fader: u16) -> u8 {
    if fader < 1366 {
        0
    } else if fader < 2731 {
        1
    } else {
        2
    }
}

fn function_quality(func: u8) -> ChordQuality {
    match func {
        1 => ChordQuality::Dom7,
        2 => ChordQuality::Min7,
        _ => ChordQuality::Maj7,
    }
}

fn function_root_offset(center: u8, func: u8, interval: u8) -> u8 {
    let center_offset = (center as u16 * interval as u16 % 12) as u8;
    match func {
        1 => (center_offset + 7) % 12,
        2 => (center_offset + 2) % 12,
        _ => center_offset,
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
        sustain_mode,
        velocity,
        led_color,
        jack_param,
        range,
        vpo,
        follow_tonic,
        _follow_scale,
        motion,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.root,
            p.division,
            p.interval,
            p.voicing,
            p.sustain,
            p.velocity,
            p.color,
            p.jack.min(JACK_COUNT - 1),
            p.range,
            p.vpo,
            p.follow_tonic,
            p.follow_scale,
            Motion::from_index(p.motion),
        )
    });

    let vel12 = velocity_12bit(velocity);

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let ticks = app.clock_ticker();
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

    let (pos0, func0, muted0, feel0) =
        storage.query(|s| (s.position_saved, s.function_saved, s.muted, s.feel));

    let glob_position = app.make_global(pos0);
    let glob_feel = app.make_global(feel0);
    // Config Interval is the start value; Shift+Long cycles from there.
    let glob_interval_idx = app.make_global(interval_param.min(3) as u8);
    let glob_btn_flash = app.make_global(0u16);
    let glob_function = app.make_global(func0);
    let glob_div = app.make_global(RESOLUTION[division.min(RESOLUTION.len() - 1)]);
    let glob_muted = app.make_global(muted0);
    let glob_latch = app.make_global(LatchLayer::Main);
    let glob_fader_moved = app.make_global(false);
    let glob_button_duck = app.make_global(0u16);
    let glob_center_idx = app.make_global(0u8);
    let glob_latched = app.make_global(false);
    let glob_drone = app.make_global(false);
    let glob_fader_dirty = app.make_global(false);
    let long_press_fired = app.make_global(false);

    if muted0 {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let fut_engine = async {
        let mut sounding: Vec<u8, SOUNDING_CAP> = Vec::new();
        let mut last_seen = ticks();
        let mut last_div_fire: u64 = u64::MAX;
        let mut stall_ms = 0u16;
        let mut prev_center: u8 = 0;
        let mut prev_func: u8 = 0;
        let mut prev_muted = muted0;
        let mut pending_fire: Option<u64> = None;
        // Arp run of the current chord, still waiting for its tick.
        let mut pending_notes: Vec<(u64, u8), SOUNDING_CAP> = Vec::new();
        // Staccato only: scheduled note-offs, half a window after each note-on.
        let mut gate_offs: Vec<(u64, u8), SOUNDING_CAP> = Vec::new();
        // Division boundaries the current chord still swallows (harmonic rhythm).
        let mut hold_boundaries: u32 = 0;
        let mut arp_vel: u16 = vel12;
        // Tick spacing of the running arp, for staccato gate lengths.
        let mut arp_spacing: u64 = 0;

        loop {
            app.delay_millis(2).await;

            // Polled, so mute silences a held chord even with the clock stopped.
            let muted = glob_muted.get();
            if muted != prev_muted {
                prev_muted = muted;
                if muted {
                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();
                    pending_fire = None;
                    pending_notes.clear();
                    gate_offs.clear();
                    if let Some(ref jack) = out_jack {
                        jack.set_value(cv_idle(range));
                    }
                }
            }

            let t = ticks();
            let drone = glob_drone.get();

            if drone {
                pending_notes.clear();
                gate_offs.clear();
                if muted {
                    last_seen = t;
                    continue;
                }

                let pos_fader = glob_position.get();
                let func_fader = glob_function.get();
                let (center, in_transition) = position_to_center_and_approach(pos_fader);
                let func = function_from_fader(func_fader);

                if center != prev_center || func != prev_func {
                    let root_midi = follow_key::root(follow_tonic, root_note);
                    let interval_semi = interval_semitones(glob_interval_idx.get() as usize);
                    let (quality, root_offset) = if in_transition {
                        (ChordQuality::Dom7, function_root_offset(center, 1, interval_semi))
                    } else {
                        (function_quality(func), function_root_offset(center, func, interval_semi))
                    };

                    let chord_root = (root_midi as u16 + root_offset as u16).min(127) as u8;
                    let notes = build_chord_voice_led(chord_root, quality, voicing, &sounding);

                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();

                    let vel = feel_velocity(
                        vel12,
                        glob_feel.get(),
                        !in_transition && func == 0,
                        die.roll(),
                    );
                    for &n in notes.iter() {
                        midi.try_send_note_on(MidiNote::from(n), vel);
                        let _ = sounding.push(n);
                    }

                    if let Some(ref jack) = out_jack {
                        jack.set_value(note_to_pitch(chord_root).as_counts(range, vpo));
                    }

                    prev_center = center;
                    prev_func = func;
                    glob_center_idx.set(center);
                    glob_button_duck.set(BUTTON_DUCK_MS);
                }

                last_seen = t;
                continue;
            }

            if t == last_seen {
                stall_ms = stall_ms.saturating_add(2);
                if stall_ms >= 250 && !sounding.is_empty() && sustain_mode != 1 {
                    for n in sounding.iter() {
                        midi.send_note_off(MidiNote::from(*n)).await;
                    }
                    sounding.clear();
                    pending_fire = None;
                    pending_notes.clear();
                    gate_offs.clear();
                    hold_boundaries = 0;
                }
                continue;
            }
            stall_ms = 0;

            if t < last_seen {
                for n in sounding.iter() {
                    midi.send_note_off(MidiNote::from(*n)).await;
                }
                sounding.clear();
                pending_fire = None;
                pending_notes.clear();
                gate_offs.clear();
                hold_boundaries = 0;
                last_seen = t;
                last_div_fire = u64::MAX;
                continue;
            }

            let div = glob_div.get().max(1) as u64;
            let boundary = t - (t % div);
            last_seen = t;

            // Drain the arp run of the current chord.
            if muted {
                pending_notes.clear();
                gate_offs.clear();
            } else {
                // Staccato: release notes whose gate has run out.
                let mut i = 0;
                while i < gate_offs.len() {
                    let (at, n) = gate_offs[i];
                    if t < at {
                        i += 1;
                        continue;
                    }
                    gate_offs.swap_remove(i);
                    if let Some(pos) = sounding.iter().position(|&s| s == n) {
                        midi.send_note_off(MidiNote::from(n)).await;
                        sounding.swap_remove(pos);
                    }
                }

                let mut i = 0;
                while i < pending_notes.len() {
                    let (at, n) = pending_notes[i];
                    if t < at {
                        i += 1;
                        continue;
                    }
                    pending_notes.swap_remove(i);
                    // Retrigger on the descent: off first so the note stays
                    // unique in `sounding`.
                    if let Some(pos) = sounding.iter().position(|&s| s == n) {
                        midi.send_note_off(MidiNote::from(n)).await;
                        sounding.swap_remove(pos);
                    }
                    // Drop a stale gate-off so it can't cut the retrigger.
                    if let Some(pos) = gate_offs.iter().position(|&(_, g)| g == n) {
                        gate_offs.swap_remove(pos);
                    }
                    midi.try_send_note_on(MidiNote::from(n), arp_vel);
                    let _ = sounding.push(n);
                    if sustain_mode == 2 {
                        let _ = gate_offs.push((t + gate_ticks(arp_spacing), n));
                    }
                }
            }

            let mut fire = false;
            if !(boundary == 0 && t < div) && boundary != last_div_fire {
                last_div_fire = boundary;
                if hold_boundaries > 0 {
                    hold_boundaries -= 1;
                } else {
                    // Feel shares its timing budget with the device-global swing.
                    let gs = get_global_config().clock.swing_amount;
                    // Parity comes off the clock grid, not the app's start.
                    let delay =
                        feel_swing_ticks(glob_feel.get(), div as u32, (boundary / div) as u32, gs)
                            as u64;
                    if delay == 0 {
                        fire = true;
                    } else {
                        pending_fire = Some(boundary + delay);
                    }
                }
            }
            if let Some(at) = pending_fire {
                if t >= at {
                    pending_fire = None;
                    fire = true;
                }
            }
            if !fire || muted {
                continue;
            }

            if glob_latched.get() {
                continue;
            }

            let pos_fader = if jack_param == JACK_IN_POS {
                let base = glob_position.get() as i32;
                let cv = in_jack.as_ref().map(|j| j.get_value() as i32 - 2047).unwrap_or(0);
                (base + cv).clamp(0, 4095) as u16
            } else {
                glob_position.get()
            };

            let func_fader = if jack_param == JACK_IN_FUNC {
                let base = glob_function.get() as i32;
                let cv = in_jack.as_ref().map(|j| j.get_value() as i32 - 2047).unwrap_or(0);
                (base + cv).clamp(0, 4095) as u16
            } else {
                glob_function.get()
            };

            let (center, in_transition) = position_to_center_and_approach(pos_fader);
            let func = function_from_fader(func_fader);

            let root_midi = follow_key::root(follow_tonic, root_note);
            let interval_semi = interval_semitones(glob_interval_idx.get() as usize);
            let (quality, root_offset) = if in_transition {
                (ChordQuality::Dom7, function_root_offset(center, 1, interval_semi))
            } else {
                (function_quality(func), function_root_offset(center, func, interval_semi))
            };

            let chord_root = (root_midi as u16 + root_offset as u16).min(127) as u8;
            let chord_root = tritone_sub_root(chord_root, quality, motion, die.roll());
            let notes = build_chord_voice_led(chord_root, quality, voicing, &sounding);

            for n in sounding.iter() {
                midi.send_note_off(MidiNote::from(*n)).await;
            }
            sounding.clear();
            pending_notes.clear();
            gate_offs.clear();

            // Tonic of a settled center carries the accent, V / ii lighten.
            let vel = feel_velocity(
                vel12,
                glob_feel.get(),
                !in_transition && func == 0,
                die.roll(),
            );
            arp_vel = vel;

            let mult = step_div_mult(motion, quality);
            hold_boundaries = mult - 1;

            let order = if motion >= Motion::Sheets {
                arp_order(notes.len())
            } else {
                Vec::new()
            };
            // Spread the run over what is left of the step (swing may have
            // eaten into it); too short for a tick per note and it stays a
            // block chord.
            let spacing = if order.is_empty() {
                0
            } else {
                let window = ((boundary / div + mult as u64) * div).saturating_sub(t);
                window / order.len() as u64
            };

            arp_spacing = spacing;

            if spacing == 0 {
                let off_at = t + gate_ticks(div * mult as u64);
                for &n in notes.iter() {
                    midi.try_send_note_on(MidiNote::from(n), vel);
                    let _ = sounding.push(n);
                    if sustain_mode == 2 {
                        let _ = gate_offs.push((off_at, n));
                    }
                }
            } else {
                for (slot, &i) in order.iter().enumerate() {
                    let n = notes[i as usize];
                    if slot == 0 {
                        midi.try_send_note_on(MidiNote::from(n), vel);
                        let _ = sounding.push(n);
                        if sustain_mode == 2 {
                            let _ = gate_offs.push((t + gate_ticks(spacing), n));
                        }
                    } else {
                        let _ = pending_notes.push((t + slot as u64 * spacing, n));
                    }
                }
            }

            if let Some(ref jack) = out_jack {
                jack.set_value(note_to_pitch(chord_root).as_counts(range, vpo));
            }

            prev_center = center;
            prev_func = func;
            glob_center_idx.set(center);
            glob_button_duck.set(BUTTON_DUCK_MS);
        }
    };

    let fut_faders = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change_at(0).await;
            let layer = glob_latch.get();
            glob_fader_moved.set(true);

            let target = match layer {
                LatchLayer::Main => glob_position.get(),
                LatchLayer::Alt => glob_function.get(),
                LatchLayer::Third => glob_feel.get(),
            };

            if let Some(v) = latch.update(faders.get_value(), layer, target) {
                match layer {
                    LatchLayer::Main => {
                        glob_position.set(v);
                    }
                    LatchLayer::Alt => {
                        glob_function.set(v);
                    }
                    LatchLayer::Third => {
                        glob_feel.set(v);
                    }
                }
                glob_fader_dirty.set(true);
            }
        }
    };

    let fut_buttons = async {
        loop {
            let (_, down_shift) = buttons.wait_for_any_down().await;
            long_press_fired.set(false);
            glob_fader_moved.set(false);
            buttons.wait_for_up(0).await;

            if long_press_fired.get() || glob_fader_moved.get() {
                continue;
            }
            if down_shift {
                glob_latched.set(!glob_latched.get());
            } else {
                glob_muted.set(!glob_muted.get());
                glob_fader_dirty.set(true);
            }
        }
    };

    let fut_long = async {
        loop {
            let (_, shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            // Third layer is button held + fader moved; don't fire on those.
            if glob_fader_moved.get() {
                continue;
            }
            if shift {
                let idx = (glob_interval_idx.get() + 1) % 4;
                glob_interval_idx.set(idx);
                glob_fader_dirty.set(true);
                leds.set_mode(0, Led::Button, LedMode::Flash(interval_color(idx), Some(2)));
                glob_btn_flash.set(BUTTON_FLASH_MS);
            } else {
                let was = glob_drone.get();
                glob_drone.set(!was);
            }
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
                st.position_saved = glob_position.get();
                st.function_saved = glob_function.get();
                st.muted = glob_muted.get();
                st.interval_idx = glob_interval_idx.get();
                st.feel = glob_feel.get();
            });
        }
    };

    let fut_leds = async {
        let mut blink_counter: u16 = 0;
        loop {
            app.delay_millis(8).await;

            let shift = buttons.is_shift_pressed();
            let pressed = buttons.is_button_pressed(0);
            let layer = if shift && !pressed {
                LatchLayer::Alt
            } else if pressed && !shift {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch.set(layer);

            let flash_left = glob_btn_flash.get();
            if flash_left > 0 {
                glob_btn_flash.set(flash_left.saturating_sub(LED_STEP_MS));
            }

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
            let app_color = params.query(|p| p.color);
            // Whole 120 deg triad rotates with the Color param.
            let center_col = center_from_app(app_color, center);
            let pos_fader = glob_position.get();
            let bright_raw = center_brightness(pos_fader, center, 3);
            let bright = if bright_raw > 170 {
                Brightness::High
            } else if bright_raw > 85 {
                Brightness::Mid
            } else {
                Brightness::Low
            };

            if !muted {
                leds.set(0, Led::Top, center_col, bright);
                leds.set(0, Led::Bottom, center_col, Brightness::Low);

                let latched = glob_latched.get();
                let droning = glob_drone.get();
                blink_counter = blink_counter.wrapping_add(8);

                if flash_left == 0 {
                    let btn_bright = if latched && (blink_counter / 250).is_multiple_of(2) {
                        Brightness::Off
                    } else if duck_active {
                        Brightness::Low
                    } else if droning {
                        Brightness::High
                    } else {
                        Brightness::Mid
                    };
                    leds.set(0, Led::Button, center_col, btn_bright);
                }
            } else {
                leds.unset(0, Led::Top);
                leds.unset(0, Led::Bottom);
                if flash_left == 0 {
                    leds.unset(0, Led::Button);
                }
            }
        }
    };

    let fut_scene = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(_) => {
                    let (p, f, m, iv, fe) = storage.query(|s| {
                        (
                            s.position_saved,
                            s.function_saved,
                            s.muted,
                            s.interval_idx,
                            s.feel,
                        )
                    });
                    glob_position.set(p);
                    glob_function.set(f);
                    glob_muted.set(m);
                    glob_interval_idx.set(iv.min(3));
                    glob_feel.set(fe);
                    let div = params.query(|p| p.division);
                    glob_div.set(RESOLUTION[div.min(RESOLUTION.len() - 1)]);
                }
                SceneEvent::SaveScene(_) => {}
            }
        }
    };

    join5(
        fut_engine,
        fut_faders,
        join3(fut_buttons, fut_long, fut_persist),
        fut_leds,
        fut_scene,
    )
    .await;
}
