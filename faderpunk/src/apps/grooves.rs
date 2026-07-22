use embassy_futures::{
    join::{join, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config,
    MidiChannel, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 11;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Reverse-swing LED feedback (white↔off), same as Heat Pump / Golden Gate.
const REVERSE_FADE_MS: u16 = 500;

/// 24 PPQN → one 16th note.
const SIXTEENTH: u32 = 6;
/// 16 sixteenths per 4/4 bar.
const STEPS_PER_BAR: u32 = 16;

const JACK_ANY: u8 = 0;
const JACK_STACKED: u8 = 1;

const NUM_GENRES: usize = 8;

/// Genre display colors (cycled briefly on Shift+long).
const GENRE_COLORS: [Color; NUM_GENRES] = [
    Color::Orange, // Dub
    Color::Yellow, // Disco
    Color::Red,    // Hip-Hop
    Color::Pink,   // House
    Color::Cyan,   // Techno
    Color::Violet, // Trip-Hop
    Color::Green,  // UK Garage
    Color::Blue,   // Dubstep
];

/// Bitmasks: bit N = 16th step N in a bar (0 = downbeat).
struct Pattern {
    kick: u16,
    snare: u16,
    /// Always-on hats for this genre.
    hats: u16,
    /// Extra kick hits revealed progressively as density rises.
    kick_fill: u16,
    /// Extra snare/ghost hits revealed progressively as density rises.
    snare_fill: u16,
    /// Extra hats revealed progressively as density rises.
    hats_fill: u16,
}

/// Oldest → newest. Indices match Shift+long cycle.
const PATTERNS: [Pattern; NUM_GENRES] = [
    // 0 Dub — sparse kick, snare 2&4, thin offbeat hats
    Pattern {
        kick: 0b0000_0001_0000_0001,  // 1 and 3
        snare: 0b0001_0000_0001_0000, // 2 and 4
        hats: 0b0100_0100_0100_0100,  // offbeat 8ths
        kick_fill: 0b0000_0100_0000_0000,
        snare_fill: 0b0000_0000_1000_0000,
        hats_fill: 0b0010_0010_0010_0010,
    },
    // 1 Disco — 4-on-floor, clap 2&4, offbeat hats
    Pattern {
        kick: 0b0001_0001_0001_0001,
        snare: 0b0001_0000_0001_0000,
        hats: 0b0100_0100_0100_0100,
        kick_fill: 0b0000_0100_0000_0100,
        snare_fill: 0b0100_0000_0100_0000,
        hats_fill: 0b1010_1010_1010_1010,
    },
    // 2 Hip-Hop — boom-bap kick syncopation, snare 2&4
    Pattern {
        kick: 0b0100_0001_0010_0001, // 1, &-of-2-ish, 3, late 4
        snare: 0b0001_0000_0001_0000,
        hats: 0b0101_0101_0101_0101,
        kick_fill: 0b0100_0000_0100_0000,
        snare_fill: 0b0000_0100_0000_0100,
        hats_fill: 0b1111_1111_1111_1111,
    },
    // 3 House — classic
    Pattern {
        kick: 0b0001_0001_0001_0001,
        snare: 0b0001_0000_0001_0000,
        hats: 0b0100_0100_0100_0100,
        kick_fill: 0b0000_0100_0000_0100,
        snare_fill: 0b0100_0000_0100_0000,
        hats_fill: 0b1111_1111_1111_1111,
    },
    // 4 Techno — 4-on-floor, dense hats, sparse clap
    Pattern {
        kick: 0b0001_0001_0001_0001,
        snare: 0b0001_0000_0000_0000, // mainly beat 4
        hats: 0b0101_0101_0101_0101,
        kick_fill: 0b0100_0100_0100_0100,
        snare_fill: 0b0000_0001_0000_0000,
        hats_fill: 0b1111_1111_1111_1111,
    },
    // 5 Trip-Hop — laid-back, sparse
    Pattern {
        kick: 0b0000_0001_0000_0001,
        snare: 0b0000_0000_0001_0000, // mostly beat 3
        hats: 0b0100_0000_0100_0000,
        kick_fill: 0b0000_0000_0001_0000,
        snare_fill: 0b0000_1000_0000_0000,
        hats_fill: 0b0100_0100_0100_0100,
    },
    // 6 UK Garage — skippy kick/hats, snare 2&4
    Pattern {
        kick: 0b1000_1001_0010_0001,
        snare: 0b0001_0000_0001_0000,
        hats: 0b0110_0100_0110_0100,
        kick_fill: 0b0010_0000_0100_0000,
        snare_fill: 0b0000_0010_0000_0100,
        hats_fill: 0b1110_1101_1110_1101,
    },
    // 7 Dubstep — half-time: kick 1, snare 3
    Pattern {
        kick: 0b0000_0000_0000_0001,
        snare: 0b0000_0001_0000_0000,
        hats: 0b0100_0100_0000_0100,
        kick_fill: 0b0000_0000_0100_0000,
        snare_fill: 0b0000_1000_0000_0000,
        hats_fill: 0b0101_0100_0101_0100,
    },
];

/// Genre names — oldest → newest; indices match Shift+long and Enum param.
const GENRE_NAMES: &[&str] = &[
    "Dub",
    "Disco",
    "Hip-Hop",
    "House",
    "Techno",
    "Trip-Hop",
    "UK Garage",
    "Dubstep",
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "Grooves",
    "Multi-genre MIDI drum grooves with swing",
    Color::Orange,
    AppIcon::Die,
)
.add_param(Param::MidiNote {
    name: "MIDI Note Kick",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel Kick",
})
.add_param(Param::MidiNote {
    name: "MIDI Note Snare",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel Snare",
})
.add_param(Param::MidiNote {
    name: "MIDI Note Hats",
})
.add_param(Param::MidiChannel {
    name: "MIDI Channel Hats",
})
.add_param(Param::Enum {
    name: "Groove",
    variants: GENRE_NAMES,
})
.add_param(Param::i32 {
    name: "Swing max %",
    min: 10,
    max: 100,
})
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Orange,
        Color::Yellow,
        Color::Pink,
        Color::Cyan,
        Color::Violet,
        Color::Green,
        Color::Blue,
        Color::Rose,
    ],
})
.add_param(Param::MidiOut);

pub struct Params {
    note_kick: MidiNote,
    midi_channel_kick: MidiChannel,
    note_snare: MidiNote,
    midi_channel_snare: MidiChannel,
    note_hats: MidiNote,
    midi_channel_hats: MidiChannel,
    genre: usize,
    swing_max_pct: i32,
    gatel: i32,
    color: Color,
    midi_out: MidiOut,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            note_kick: MidiNote::from_value(values[0]),
            midi_channel_kick: MidiChannel::from_value(values[1]),
            note_snare: MidiNote::from_value(values[2]),
            midi_channel_snare: MidiChannel::from_value(values[3]),
            note_hats: MidiNote::from_value(values[4]),
            midi_channel_hats: MidiChannel::from_value(values[5]),
            genre: usize::from_value(values[6]).min(NUM_GENRES - 1),
            swing_max_pct: i32::from_value(values[7]).clamp(10, 100),
            gatel: i32::from_value(values[8]),
            color: Color::from_value(values[9]),
            midi_out: MidiOut::from_value(values[10]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.note_kick.into()).unwrap();
        vec.push(self.midi_channel_kick.into()).unwrap();
        vec.push(self.note_snare.into()).unwrap();
        vec.push(self.midi_channel_snare.into()).unwrap();
        vec.push(self.note_hats.into()).unwrap();
        vec.push(self.midi_channel_hats.into()).unwrap();
        vec.push(self.genre.into()).unwrap();
        vec.push(self.swing_max_pct.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    swing: u16,
    /// Groove density: progressively reveals extra kick/snare/hat hits
    /// across the whole pattern (not just hats) as this rises.
    density: u16,
    jack_mode: u8,
    reversed: bool,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            // ~1 tick of swing — audible default (see swing_phase)
            swing: 2000,
            density: 2048,
            jack_mode: JACK_ANY,
            reversed: false,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

fn bit_set(mask: u16, step: u32) -> bool {
    mask & (1u16 << (step % STEPS_PER_BAR)) != 0
}

/// MPC-style: delay odd 16ths by 0..=max_delay PPQN ticks.
/// `swing_max_pct` (10–100) sets how far "fader top" goes as % of a 16th note
/// (50% ≈ classic MPC; 100% = full 16th late).
fn swing_phase(step: u32, swing: u16, reversed: bool, swing_max_pct: i32) -> u32 {
    let pct = swing_max_pct.clamp(10, 100) as u32;
    let max_delay = ((SIXTEENTH * pct) / 100).max(1);
    let delay = ((swing as u32) * max_delay + 2047) / 4095;
    let odd = step % 2 == 1;
    let delay_this = if reversed { !odd } else { odd };
    if delay_this {
        delay
    } else {
        0
    }
}

/// Continuous "groove density" reveal for one voice's fill mask. Returns
/// `Some(frac)` if `step`'s fill bit should sound at this density, where
/// `frac` is 0..=255: 255 = fully revealed, lower = still fading in as the
/// fader crosses this bit's reveal point. Bits are revealed in step order,
/// one at a time, so every notch of fader movement changes *something* —
/// no hard density-zone jumps.
fn fill_reveal(fill: u16, density: u16, step: u32) -> Option<u8> {
    let bit = 1u16 << (step % STEPS_PER_BAR);
    if fill & bit == 0 {
        return None;
    }
    let total = fill.count_ones();
    if total == 0 {
        return None;
    }
    let mut rank = 0u32;
    for i in 0..(step % STEPS_PER_BAR) {
        if fill & (1u16 << i) != 0 {
            rank += 1;
        }
    }
    // Fixed-point (x256) count of fill bits revealed so far at this density.
    let revealed_scaled = (density as u32) * total * 256 / 4095;
    let revealed_count = revealed_scaled / 256;
    let frac = (revealed_scaled % 256) as u8;
    if rank < revealed_count {
        Some(255)
    } else if rank == revealed_count && frac > 0 {
        Some(frac)
    } else {
        None
    }
}

/// Scales a velocity percent between `quiet` (a ghost note the instant it's
/// revealed) and `full` (fully faded in) as `frac` (0..=255) rises.
fn ghost_vel_pct(frac: u8, quiet: u16, full: u16) -> u16 {
    quiet + ((full - quiet) as u32 * frac as u32 / 255) as u16
}

/// Extra micro-timing push for ghost-only steps (no core hit landing on the
/// same step): as density rises, revealed ghosts drag a little further
/// behind the grid, like a drummer digging into the pocket. Capped at a
/// fraction of the swing tick budget so it can never overtake the next step.
fn ghost_drag_ticks(density: u16) -> u32 {
    (density as u32 * 2) / 4095
}

fn midi_vel(mult: u16) -> u16 {
    // mult is 0..=100 "percent" of full scale
    ((4095u32 * mult as u32) / 100).min(4095) as u16
}

fn any_pulse_level(kick: bool, snare: bool, hats: bool) -> u16 {
    let mut level = 0u16;
    if hats {
        level = level.max(1400);
    }
    if snare {
        level = level.max(2600);
    }
    if kick {
        level = level.max(4095);
    }
    level
}

fn stacked_pulse_level(kick: bool, snare: bool, hats: bool) -> u16 {
    // ~1V / 2V / 4V on 0–10V (4095 ≈ 10V)
    let mut units = 0u16;
    if hats {
        units += 1;
    }
    if snare {
        units += 2;
    }
    if kick {
        units += 4;
    }
    ((units as u32 * 4095) / 10).min(4095) as u16
}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(
        app.app_id,
        app.layout_id,
        Params {
            note_kick: MidiNote::from(36),
            midi_channel_kick: MidiChannel::default(),
            note_snare: MidiNote::from(38),
            midi_channel_snare: MidiChannel::default(),
            note_hats: MidiNote::from(42),
            midi_channel_hats: MidiChannel::default(),
            genre: 3, // House
            swing_max_pct: 50,
            gatel: 40,
            color: Color::Orange,
            midi_out: MidiOut::default(),
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
    let (
        midi_out,
        note_kick,
        note_snare,
        note_hats,
        midi_channel_kick,
        midi_channel_snare,
        midi_channel_hats,
        genre,
        swing_max_pct,
        gatel,
        led_color,
    ) = params.query(|p| {
        (
            p.midi_out,
            p.note_kick,
            p.note_snare,
            p.note_hats,
            p.midi_channel_kick,
            p.midi_channel_snare,
            p.midi_channel_hats,
            p.genre.min(NUM_GENRES - 1),
            p.swing_max_pct.clamp(10, 100),
            p.gatel,
            p.color,
        )
    });

    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi_kick = app.use_midi_output(midi_out, midi_channel_kick, false);
    let midi_snare = app.use_midi_output(midi_out, midi_channel_snare, false);
    let midi_hats = app.use_midi_output(midi_out, midi_channel_hats, false);
    let jack = app.make_out_jack(0, Range::_0_10V).await;
    jack.set_value(0);

    let (swing, density, jack_mode, reversed, muted) =
        storage.query(|s| (s.swing, s.density, s.jack_mode, s.reversed, s.muted));

    let glob_swing = app.make_global(swing);
    let glob_swing_max = app.make_global(swing_max_pct);
    let glob_density = app.make_global(density);
    let glob_jack_mode = app.make_global(jack_mode);
    let glob_reversed = app.make_global(reversed);
    let glob_genre = app.make_global(genre);
    let glob_muted = app.make_global(muted);
    let glob_reset = app.make_global(false);
    let long_press_fired = app.make_global(false);
    let glob_fader_moved = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_reverse_fade = app.make_global(0u16);
    let glob_reverse_fade_up = app.make_global(false);
    let glob_genre_flash = app.make_global(0u16);

    // Clear any hanging notes from a prior respawn.
    midi_kick.send_note_off(note_kick).await;
    midi_snare.send_note_off(note_snare).await;
    midi_hats.send_note_off(note_hats).await;

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let fut_clock = async {
        let mut origin: u32 = 0;
        let mut origin_set = false;
        let mut kick_on = false;
        let mut snare_on = false;
        let mut hats_on = false;
        let mut gate_off_at: Option<u32> = None;
        // Fire-once guard per 16th slot; u32::MAX = nothing fired yet.
        let mut last_fired_slot = u32::MAX;
        let gate_len = (SIXTEENTH as i32 * gatel / 100).clamp(1, (SIXTEENTH as i32) - 1) as u32;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset | ClockEvent::Stop => {
                    if kick_on {
                        midi_kick.send_note_off(note_kick).await;
                        kick_on = false;
                    }
                    if snare_on {
                        midi_snare.send_note_off(note_snare).await;
                        snare_on = false;
                    }
                    if hats_on {
                        midi_hats.send_note_off(note_hats).await;
                        hats_on = false;
                    }
                    jack.set_value(0);
                    gate_off_at = None;
                    origin_set = false;
                    last_fired_slot = u32::MAX;
                    glob_reset.set(false);
                    leds.unset(0, Led::Top);
                    leds.unset(0, Led::Bottom);
                }
                ClockEvent::Tick => {
                    let clkn = ticks() as u32;

                    if !origin_set || glob_reset.get() {
                        origin = clkn;
                        origin_set = true;
                        last_fired_slot = u32::MAX;
                        glob_reset.set(false);
                    }

                    let pos = clkn.wrapping_sub(origin);
                    // Absolute 16th slot since origin (not wrapped) for the fire-once guard.
                    let slot = pos / SIXTEENTH;
                    let step = (pos / SIXTEENTH) % STEPS_PER_BAR;
                    let phase = pos % SIXTEENTH;
                    let delay = swing_phase(
                        step,
                        glob_swing.get(),
                        glob_reversed.get(),
                        glob_swing_max.get(),
                    )
                    .min(SIXTEENTH - 1);

                    // Note / jack off
                    if let Some(off_at) = gate_off_at {
                        if clkn >= off_at {
                            if kick_on {
                                midi_kick.send_note_off(note_kick).await;
                                kick_on = false;
                            }
                            if snare_on {
                                midi_snare.send_note_off(note_snare).await;
                                snare_on = false;
                            }
                            if hats_on {
                                midi_hats.send_note_off(note_hats).await;
                                hats_on = false;
                            }
                            jack.set_value(0);
                            gate_off_at = None;
                            leds.set(0, Led::Bottom, led_color, Brightness::Off);
                        }
                    }

                    // Fire-once guard: a swing/density change mid-window
                    // can't skip a step or fire it twice.
                    if slot != last_fired_slot && !glob_muted.get() {
                        let density = glob_density.get();
                        let genre = glob_genre.get().min(NUM_GENRES - 1);
                        let pat = &PATTERNS[genre];

                        let kick_core = bit_set(pat.kick, step);
                        let snare_core = bit_set(pat.snare, step);
                        let hats_core = bit_set(pat.hats, step);
                        // `Some(frac)`: this step's fill bit for that voice is
                        // being progressively revealed by the density fader —
                        // frac (0..=255) is how far in it's faded (continuum,
                        // no hard zone jumps).
                        let kick_ghost = fill_reveal(pat.kick_fill, density, step);
                        let snare_ghost = fill_reveal(pat.snare_fill, density, step);
                        let hats_ghost = fill_reveal(pat.hats_fill, density, step);

                        let core_hit = kick_core || snare_core || hats_core;
                        let any_ghost =
                            kick_ghost.is_some() || snare_ghost.is_some() || hats_ghost.is_some();
                        // Ghost-only steps (no core hit) drag a little behind
                        // the grid as density rises — a looser, more human
                        // pocket — but never displace a core hit's timing.
                        let required_delay = if core_hit || !any_ghost {
                            delay
                        } else {
                            (delay + ghost_drag_ticks(density)).min(SIXTEENTH - 1)
                        };

                        if phase >= required_delay {
                            last_fired_slot = slot;

                            let do_kick = kick_core || kick_ghost.is_some();
                            let do_snare = snare_core || snare_ghost.is_some();
                            let do_hats = hats_core || hats_ghost.is_some();

                            // A late-swung previous hit may still be sounding
                            // (its gate-off lands after this step's start):
                            // flush note-offs before re-triggering to avoid
                            // overlapping note-ons on the same key.
                            if (do_kick || do_snare || do_hats) && gate_off_at.is_some() {
                                if kick_on {
                                    midi_kick.send_note_off(note_kick).await;
                                    kick_on = false;
                                }
                                if snare_on {
                                    midi_snare.send_note_off(note_snare).await;
                                    snare_on = false;
                                }
                                if hats_on {
                                    midi_hats.send_note_off(note_hats).await;
                                    hats_on = false;
                                }
                                gate_off_at = None;
                            }

                            if do_kick {
                                let v = match kick_ghost {
                                    Some(frac) if !kick_core => ghost_vel_pct(frac, 20, 75),
                                    _ => 100,
                                };
                                midi_kick.send_note_on(note_kick, midi_vel(v)).await;
                                kick_on = true;
                            }
                            if do_snare {
                                let v = match snare_ghost {
                                    Some(frac) if !snare_core => ghost_vel_pct(frac, 18, 65),
                                    _ => 90,
                                };
                                midi_snare.send_note_on(note_snare, midi_vel(v)).await;
                                snare_on = true;
                            }
                            if do_hats {
                                let v = match hats_ghost {
                                    Some(frac) if !hats_core => ghost_vel_pct(frac, 12, 55),
                                    _ => 55,
                                };
                                midi_hats.send_note_on(note_hats, midi_vel(v)).await;
                                hats_on = true;
                            }

                            if do_kick || do_snare || do_hats {
                                let level = if glob_jack_mode.get() == JACK_STACKED {
                                    stacked_pulse_level(do_kick, do_snare, do_hats)
                                } else {
                                    any_pulse_level(do_kick, do_snare, do_hats)
                                };
                                jack.set_value(level);
                                gate_off_at = Some(clkn.wrapping_add(gate_len));
                                leds.set(0, Led::Bottom, led_color, Brightness::High);
                            }
                        }
                    }

                    // Top LED: bar progress by default; swing amount while
                    // Shift is held (that's the layer it now controls).
                    match glob_latch_layer.get() {
                        LatchLayer::Main => {
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom(((step * 255) / STEPS_PER_BAR) as u8),
                            );
                        }
                        LatchLayer::Alt => {
                            let s = glob_swing.get();
                            leds.set(0, Led::Top, Color::Red, Brightness::Custom((s / 16) as u8));
                        }
                        LatchLayer::Third => {
                            let mode_color = if glob_jack_mode.get() == JACK_STACKED {
                                Color::Violet
                            } else {
                                Color::Yellow
                            };
                            leds.set(0, Led::Top, mode_color, LED_BRIGHTNESS);
                        }
                    }
                }
                _ => {}
            }
        }
    };

    let fut_buttons = async {
        loop {
            buttons.wait_for_any_down().await;
            if buttons.is_shift_pressed() {
                long_press_fired.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    // Shift + short: reverse swing
                    let reversed = glob_reversed.toggle();
                    storage.modify_and_save(|s| s.reversed = reversed);
                    glob_reverse_fade_up.set(!reversed);
                    glob_reverse_fade.set(REVERSE_FADE_MS);
                }
            } else {
                long_press_fired.set(false);
                glob_fader_moved.set(false);
                buttons.wait_for_up(0).await;
                if !long_press_fired.get() {
                    // Short: reset to downbeat
                    glob_reset.set(true);
                } else if !glob_fader_moved.get() {
                    // Long (no fader move): mute
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| s.muted = muted);
                    if muted {
                        leds.unset(0, Led::Button);
                        jack.set_value(0);
                        midi_kick.send_note_off(note_kick).await;
                        midi_snare.send_note_off(note_snare).await;
                        midi_hats.send_note_off(note_hats).await;
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
            }
        }
    };

    let long_press = async {
        loop {
            let (_, is_shift) = buttons.wait_for_any_long_press().await;
            long_press_fired.set(true);
            if is_shift {
                // Shift + long: cycle genre (oldest → newest); persist to params
                let next = (glob_genre.get() + 1) % NUM_GENRES;
                glob_genre.set(next);
                params.update(|p| p.genre = next).await;
                glob_genre_flash.set(300);
                if !glob_muted.get() {
                    leds.set(0, Led::Button, GENRE_COLORS[next], Brightness::High);
                }
            }
        }
    };

    let fut_faders = async {
        let mut latch = app.make_latch(faders.get_value());
        loop {
            faders.wait_for_change_at(0).await;
            let latch_layer = glob_latch_layer.get();

            if buttons.is_button_pressed(0) {
                glob_fader_moved.set(true);
            }

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.density),
                LatchLayer::Alt => storage.query(|s| s.swing),
                LatchLayer::Third => {
                    if storage.query(|s| s.jack_mode) == JACK_STACKED {
                        3072
                    } else {
                        1024
                    }
                }
            };

            if let Some(new_value) = latch.update(faders.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        glob_density.set(new_value);
                        storage.modify_and_save(|s| s.density = new_value);
                    }
                    LatchLayer::Alt => {
                        glob_swing.set(new_value);
                        storage.modify_and_save(|s| s.swing = new_value);
                    }
                    LatchLayer::Third => {
                        glob_fader_moved.set(true);
                        let mode = if new_value > 2048 {
                            JACK_STACKED
                        } else {
                            JACK_ANY
                        };
                        glob_jack_mode.set(mode);
                        storage.modify_and_save(|s| s.jack_mode = mode);
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (swing, density, jack_mode, reversed, muted) =
                        storage.query(|s| (s.swing, s.density, s.jack_mode, s.reversed, s.muted));
                    glob_swing.set(swing);
                    glob_density.set(density);
                    glob_jack_mode.set(jack_mode);
                    glob_reversed.set(reversed);
                    glob_muted.set(muted);
                    // Genre lives in params (Configurator); refresh from there.
                    glob_genre.set(params.query(|p| p.genre.min(NUM_GENRES - 1)));
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    let shift = async {
        loop {
            app.delay_millis(1).await;
            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);

            // Reverse fade overrides button LED
            let fade_left = glob_reverse_fade.get();
            if fade_left > 0 {
                let elapsed = REVERSE_FADE_MS.saturating_sub(fade_left);
                let bright = if glob_reverse_fade_up.get() {
                    ((elapsed as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                } else {
                    (((REVERSE_FADE_MS - elapsed) as u32 * 255) / REVERSE_FADE_MS as u32) as u8
                };
                leds.set(0, Led::Button, Color::White, Brightness::Custom(bright));
                glob_reverse_fade.set(fade_left.saturating_sub(1));
                if fade_left == 1 {
                    // Don't leave the LED stuck white when muted.
                    if glob_muted.get() {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
            }

            // Genre flash counts down independently of the reverse fade so it
            // can't stall behind it; the restore is skipped while a fade is
            // still animating (the fade's own end handler restores the LED).
            let flash_left = glob_genre_flash.get();
            if flash_left > 0 {
                let left = flash_left.saturating_sub(1);
                glob_genre_flash.set(left);
                if left == 0 && glob_reverse_fade.get() == 0 {
                    if glob_muted.get() {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
            }
        }
    };

    join(
        join5(fut_clock, fut_buttons, fut_faders, scene_handler, shift),
        long_press,
    )
    .await;
}
