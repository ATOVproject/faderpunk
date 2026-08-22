//! Venn — dual Euclidean layers combined with boolean logic (OR/AND/XOR/Accnt).
//!
//! Gates + generative MIDI melody: Length-A phase walks a scale-degree arch
//! within Extent; Ch2 = same line + diatonic Interval. Scale shape follows the
//! device quantizer key (Note A = degree 0). Inspired by OXI ONE MKII GEN page 2
//! (eLen/ePul + Logic). Rotation is fixed at 0 (live slots used for Extent /
//! Interval). Distinct from Euclid and GenSeq.

use embassy_futures::{
    join::{join, join5},
    select::{select, select3, Either},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue, latch::LatchLayer, utils::euclidean_at, AppIcon, Brightness, ClockDivision,
    Color, Config, Key, MidiChannel, MidiNote, MidiOut, Param, Value, APP_MAX_PARAMS,
};
use midly::num::u7;
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};
use crate::tasks::leds::LedMode;

use self::led_fx::hsv_to_rgb;
use smart_leds::RGB8;

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 10;

/// Interval B select labels (index == diatonic steps above the line).
/// Wire/FRAM indices 0..=12 stay stable; live Interval is also on Alt F1.
const INTERVAL_B_VARIANTS: &[&str] = &[
    "Unison", "m2", "M2", "m3", "M3", "P4", "TT", "P5", "m6", "M6", "m7", "M7", "Octave",
];

/// Max melodic span above Note A in **scale degrees** when Extent fader is full.
const EXTENT_MAX_DEGREES: u8 = 24;
/// Max Interval B above the melodic line in **scale degrees** (0..=12).
const INTERVAL_MAX_DEGREES: u8 = 12;

const LED_BRIGHTNESS: Brightness = Brightness::Mid;
/// Button hit soft-dip duration (1 ms ticks). Short so dense hits still blink
/// (long hold-offs stay stuck at 62% and look like no pulse).
const BUTTON_HIT_FLASH_MS: u16 = 40;
/// Hit remain light: dim 38% → keep 62% (no black flash).
const BUTTON_HIT_REMAIN_PCT: u8 = 62;
/// Hold off Main button paint so LedMode::Flash (logic cycle) can finish.
const BUTTON_FLASH_MS: u16 = 550;
/// eInv gesture LED feedback (white↔off), same as Heat Pump invert.
const EINVERT_FADE_MS: u16 = 500;
/// Floor / span for pulse-density brightness on buttons (Main).
const BTN_BRIGHT_FLOOR: u8 = 90;
const BTN_BRIGHT_SPAN: u8 = 165;
/// Floor for Alt rotation / Third length Top+Button meters (rot=0 still visible).
const LAYER_LED_FLOOR: u8 = 40;
/// Ignore tiny fader noise when deciding Btn2 tap-mute vs Third scrub.
const FADER_MOVE_THRESH: u16 = 64;
// TODO(shared-utils): this is the first 12 entries of libfp::utils::CLOCK_DIVISIONS.
// Drop the local copy once that helper is on this branch's base. Venn indexes an
// already-computed division, so it needs the constant only, not `division_at`.
const RESOLUTION: [u32; 12] = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];
/// Labels for `RESOLUTION` (24 PPQN, bar = 96 ticks): index 0 = 4/1 … 11 = 1/32T.
const DIVISION_VARIANTS: &[&str] = &[
    "4/1", "2/1", "1/1", "1/2", "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T",
];

/// Boolean combine modes for the two Euclidean layers.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Logic {
    Or = 0,
    And = 1,
    Xor = 2,
    Accnt = 3,
}

impl Logic {
    fn from_u8(v: u8) -> Self {
        match v % 4 {
            0 => Self::Or,
            1 => Self::And,
            2 => Self::Xor,
            _ => Self::Accnt,
        }
    }

    fn next(self) -> Self {
        Self::from_u8(self as u8 + 1)
    }

    fn color(self) -> Color {
        match self {
            Self::Or => Color::Green,
            Self::And => Color::Yellow,
            Self::Xor => Color::Red,
            Self::Accnt => Color::White,
        }
    }
}

/// Auto-complement for Layer B LEDs / Third hue endpoint from a base color.
fn complement_color(c: Color) -> Color {
    match c {
        Color::Blue | Color::SkyBlue | Color::LightBlue => Color::Orange,
        Color::Green | Color::Lime | Color::PaleGreen => Color::Violet,
        Color::Rose | Color::Salmon => Color::Yellow,
        Color::Orange | Color::Sand => Color::Blue,
        Color::Cyan => Color::Pink,
        Color::Pink => Color::Cyan,
        Color::Violet => Color::Green,
        Color::Yellow => Color::Rose,
        Color::White | Color::Custom(_, _, _) => Color::Pink,
        Color::Red => Color::Cyan,
    }
}

fn color_rgb(c: Color) -> RGB8 {
    RGB8::from(c)
}

/// Approximate hue in degrees (0..360). White / near-grey → 0.
fn color_hue(c: Color) -> u16 {
    let RGB8 { r, g, b } = color_rgb(c);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max == 0 || max - min < 8 {
        return 0;
    }
    let d = (max - min) as i32;
    let (r, g, b, max) = (r as i32, g as i32, b as i32, max as i32);
    let h = if max == r {
        ((g - b) * 60) / d
    } else if max == g {
        120 + ((b - r) * 60) / d
    } else {
        240 + ((r - g) * 60) / d
    };
    ((h % 360) + 360) as u16 % 360
}

fn shortest_hue_lerp(from: u16, to: u16, t: u8) -> u16 {
    let from = (from % 360) as i16;
    let to = (to % 360) as i16;
    let mut d = to - from;
    if d > 180 {
        d -= 360;
    } else if d < -180 {
        d += 360;
    }
    let h = from + (d * i16::from(t)) / 255;
    (h.rem_euclid(360)) as u16
}

fn rgb_lerp(a: Color, b: Color, t: u8) -> Color {
    let a = color_rgb(a);
    let b = color_rgb(b);
    let t = u16::from(t);
    Color::Custom(
        ((u16::from(a.r) * (255 - t) + u16::from(b.r) * t) / 255) as u8,
        ((u16::from(a.g) * (255 - t) + u16::from(b.g) * t) / 255) as u8,
        ((u16::from(a.b) * (255 - t) + u16::from(b.b) * t) / 255) as u8,
    )
}

/// Length 2..=32 → shortest hue path from `from` toward its complement (`to`).
/// White bases RGB-lerp (no meaningful hue).
fn length_grad_color(len: u8, from: Color, to: Color) -> Color {
    let t = ((((len.clamp(2, 32) - 2) as u16) * 255) / 30) as u8;
    if matches!(from, Color::White) {
        return rgb_lerp(from, to, t);
    }
    let h = shortest_hue_lerp(color_hue(from), color_hue(to), t);
    let (r, g, b) = hsv_to_rgb(h);
    Color::Custom(r, g, b)
}

fn length_from_fader(v: u16) -> u8 {
    // 2..=32 (Bjorklund table supports 2–32 steps)
    ((v as u32 * 31 / 4095) as u8).saturating_add(2).min(32)
}

fn pulses_from_fader(v: u16, len: u8) -> u8 {
    let len = len.max(1);
    ((v as u32 * len as u32 / 4095) as u8).min(len)
}

fn extent_from_fader(v: u16) -> u8 {
    ((v as u32 * EXTENT_MAX_DEGREES as u32) / 4095) as u8
}

fn interval_from_fader(v: u16) -> u8 {
    ((v as u32 * INTERVAL_MAX_DEGREES as u32) / 4095) as u8
}

/// Pitch-class offsets from tonic (0 = tonic), MSB layout via [`Key::as_u16_key`].
fn scale_pcs(key: Key) -> Vec<u8, 12> {
    let mask = if key == Key::Off {
        Key::Chromatic.as_u16_key()
    } else {
        key.as_u16_key()
    };
    let mut out = Vec::new();
    for i in 0..12u8 {
        if (mask >> (11 - i)) & 1 != 0 {
            let _ = out.push(i);
        }
    }
    if out.is_empty() {
        let _ = out.push(0);
    }
    out
}

/// Semitone offset from tonic for absolute scale degree `d` (0 = tonic).
fn degree_semis(pcs: &[u8], d: u16) -> u16 {
    let n = pcs.len().max(1) as u16;
    let oct = d / n;
    let i = (d % n) as usize;
    oct * 12 + u16::from(pcs[i])
}

/// Note A as degree 0; climb `degree` steps in the device key (shape from tonic = Note A).
fn midi_at_degree(root: MidiNote, pcs: &[u8], degree: u16) -> MidiNote {
    let root_u = u7::from(root).as_int();
    let off0 = degree_semis(pcs, 0);
    let off = degree_semis(pcs, degree);
    let midi = (u16::from(root_u) + off.saturating_sub(off0)).min(127) as u8;
    MidiNote::from(midi)
}

/// Arch contour through Length-A into 0..=extent scale degrees (flat if extent=0).
fn melody_degree(step: u32, len: u8, extent: u8) -> u8 {
    if extent == 0 || len <= 1 {
        return 0;
    }
    let last = (len as u32 - 1).max(1);
    let phase = step % len as u32;
    // Rise to midpoint, fall back — Contura-style arch (contour 0).
    let half = last / 2;
    let t = if half == 0 {
        0
    } else if phase <= half {
        (phase * extent as u32) / half
    } else {
        let down = phase - half;
        let rem = (last - half).max(1);
        extent as u32 - (down * extent as u32) / rem
    };
    t.min(extent as u32) as u8
}

fn extent_meter_bright(extent: u8) -> u8 {
    let span = (255u16 - LAYER_LED_FLOOR as u16).max(1);
    let t = ((extent as u16 * span) / EXTENT_MAX_DEGREES as u16).min(span);
    (LAYER_LED_FLOOR as u16 + t) as u8
}

fn interval_meter_bright(interval: u8) -> u8 {
    let span = (255u16 - LAYER_LED_FLOOR as u16).max(1);
    let t = ((interval as u16 * span) / INTERVAL_MAX_DEGREES as u16).min(span);
    (LAYER_LED_FLOOR as u16 + t) as u8
}

/// Button base brightness from fill density (pulses/length) — denser = brighter.
fn button_density_bright(pulses: u8, len: u8) -> u8 {
    let len = len.max(1) as u16;
    let fill = (pulses as u16 * 255) / len;
    BTN_BRIGHT_FLOOR + ((fill * BTN_BRIGHT_SPAN as u16) / 255) as u8
}

fn hit_dip_bright(base: u8) -> u8 {
    ((base as u16 * BUTTON_HIT_REMAIN_PCT as u16) / 100).min(255) as u8
}

/// Length meter: 2..=32 → floor..=255.
fn len_meter_bright(len: u8) -> u8 {
    let len = len.clamp(2, 32) as u16;
    let span = (255u16 - LAYER_LED_FLOOR as u16).max(1);
    let t = (((len - 2) * span) / 30).min(span);
    (LAYER_LED_FLOOR as u16 + t) as u8
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Venn",
    "Dual Euclidean layers with scale-aware melody (Extent / Interval)",
    Color::Cyan,
    AppIcon::Euclid,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote {
    name: "MIDI Note A",
})
.add_param(Param::Enum {
    name: "Interval B",
    variants: INTERVAL_B_VARIANTS,
})
.add_param(Param::MidiOut)
.add_param(Param::Enum {
    name: "Division",
    variants: DIVISION_VARIANTS,
})
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
.add_param(Param::i32 {
    name: "Prob %",
    min: 0,
    max: 100,
})
.add_param(Param::i32 {
    name: "Humanize",
    min: 0,
    max: 100,
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
// Venn already follows the device scale unconditionally; this adds the tonic,
// which turns the global Tonic fader into a live transpose. No scale-follow
// counterpart is needed here.
.add_param(Param::bool {
    name: "Follow device tonic",
});

pub struct Params {
    midi_channel: MidiChannel,
    note_a: MidiNote,
    /// Index into `INTERVAL_B_VARIANTS` (0 = Unison … 12 = Octave).
    /// Seeds live Interval; Alt F1 overrides via storage.
    interval_b: usize,
    midi_out: MidiOut,
    /// Index into `DIVISION_VARIANTS` / `RESOLUTION`.
    division: usize,
    gatel: i32,
    prob: i32,
    vel: i32,
    color: Color,
    follow_tonic: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            note_a: MidiNote::from(32),
            // P5 — close intervals like m2 make the two voices hard to tell apart,
            // which also hides the logic mode.
            interval_b: 7,
            midi_out: MidiOut::default(),
            division: 8, // 1/16
            gatel: 50,
            prob: 100,
            vel: 0,
            color: Color::Cyan,
            follow_tonic: true,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Pre-follow blobs had nine slots; accept them and default the flag.
        if values.len() < 9 {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note_a: MidiNote::from_value(values[1]),
            interval_b: usize::from_value(values[2]).min(INTERVAL_B_VARIANTS.len() - 1),
            midi_out: MidiOut::from_value(values[3]),
            // Migrate legacy i32 Division 1..=12 → enum index 0..=11.
            division: match values[4] {
                Value::i32(n) => (n.clamp(1, 12) as usize).saturating_sub(1),
                _ => usize::from_value(values[4]).min(DIVISION_VARIANTS.len() - 1),
            },
            gatel: i32::from_value(values[5]),
            prob: i32::from_value(values[6]),
            vel: i32::from_value(values[7]),
            color: Color::from_value(values[8]),
            follow_tonic: values.len() < 10 || bool::from_value(values[9]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note_a.into()).unwrap();
        vec.push(self.interval_b.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.division.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.prob.into()).unwrap();
        vec.push(self.vel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.follow_tonic.into()).unwrap();
        vec
    }
}

/// Fader layout:
///   F0 Main=pulses_a  Alt=extent  Third=len_a
///   F1 Main=pulses_b  Alt=interval  Third=len_b
///   Rotation fixed at 0 (slots freed for melody Extent / Interval).
///
/// Buttons:
///   Btn0/Btn1 hold = Third (lengths); Btn1 tap (no fader move) = mute
///   Shift+Btn1 = cycle Logic OR→AND→XOR→Accnt
///   Shift+Btn2 = toggle eInv (post-logic shadow); white↔none LED fade
///
/// Outputs:
///   Jack0 = logic result + melodic line (Note A + step→extent)
///   Jack1 = Layer B + line + Interval (Accnt: coincidences only)
#[derive(Serialize, Deserialize)]
pub struct Storage {
    pulses_a: u16,
    pulses_b: u16,
    /// Was rot_a — fader 0..=4095 → 0..=24 semitone melodic span.
    extent: u16,
    /// Was rot_b — fader 0..=4095 → 0..=12 semitone interval above the line.
    interval: u16,
    len_a: u16,
    len_b: u16,
    logic: u8,
    einv: bool,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            // Mid-ish defaults: length 16, pulses ~7 / ~3
            pulses_a: 1792,
            pulses_b: 768,
            // ~octave of melodic span; P5 interval (7/12 of fader)
            extent: 2048,
            interval: 2389,
            len_a: 1840, // ~16
            len_b: 1840,
            logic: Logic::Or as u8,
            einv: false,
            muted: false,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16 / CHANNELS)]
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

fn div_from_param(division: usize) -> u32 {
    RESOLUTION[division.min(RESOLUTION.len() - 1)]
}

fn midi_velocity(base: u16, vel_pct: i32, die_roll: u16) -> u16 {
    if vel_pct <= 0 {
        return base;
    }
    let span = (base as u32 * vel_pct as u32 / 100) as u16;
    let half = span / 2;
    let offset = (die_roll as u32 * span as u32 / 4095) as i32 - half as i32;
    (base as i32 + offset).clamp(1, 4095) as u16
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let die = app.use_die();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let (midi_out, midi_chan, note_a, interval_b, division, gatel, prob, vel, color_a, follow_tonic) =
        params.query(|p| {
            (
                p.midi_out,
                p.midi_channel,
                p.note_a,
                p.interval_b,
                p.division,
                p.gatel,
                p.prob,
                p.vel,
                p.color,
                p.follow_tonic,
            )
        });
    let color_b = complement_color(color_a);

    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let jack = [
        app.make_gate_jack(0, 4095).await,
        app.make_gate_jack(1, 4095).await,
    ];

    let glob_muted = app.make_global(false);
    let glob_einv = app.make_global(false);
    let glob_logic = app.make_global(Logic::Or as u8);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let len_a_glob = app.make_global(16u8);
    let len_b_glob = app.make_global(16u8);
    let pulses_a_glob = app.make_global(7u8);
    let pulses_b_glob = app.make_global(3u8);
    let extent_glob = app.make_global(12u8);
    let interval_glob = app.make_global(1u8);
    let sounding_a_glob = app.make_global(note_a);
    let sounding_b_glob = app.make_global(note_a + MidiNote::from(1));
    // Main button pulse flashes (1 ms countdown while Flash plays).
    let glob_btn_a_flash = app.make_global(0u16);
    let glob_btn_b_flash = app.make_global(0u16);
    let glob_btn_flash = app.make_global(0u16);
    // Remaining ms of eInv LED fade; 0 = inactive.
    let glob_einv_fade = app.make_global(0u16);
    // true = none→white, false = white→none.
    let glob_einv_fade_up = app.make_global(false);
    let glob_fader_moved = app.make_global(false);
    let glob_ticks = app.make_global(0u64);
    let glob_clock_halt = app.make_global(false);

    let div = div_from_param(division);

    // Load storage → globals
    {
        let s = storage.query(|s| {
            (
                s.pulses_a,
                s.pulses_b,
                s.extent,
                s.interval,
                s.len_a,
                s.len_b,
                s.einv,
                s.muted,
            )
        });
        let (pa, pb, ext, iv, la, lb, einv, muted) = s;
        let len_a = length_from_fader(la);
        let len_b = length_from_fader(lb);
        len_a_glob.set(len_a);
        len_b_glob.set(len_b);
        pulses_a_glob.set(pulses_from_fader(pa, len_a));
        pulses_b_glob.set(pulses_from_fader(pb, len_b));
        extent_glob.set(extent_from_fader(ext));
        interval_glob.set(interval_from_fader(iv));
        glob_einv.set(einv);
        glob_muted.set(muted);
    }
    // `run` is restarted whenever a param changes, so the stored mode has to be
    // restored here — forcing OR made every configurator edit reset the logic.
    glob_logic.set(storage.query(|s| Logic::from_u8(s.logic) as u8));
    // Host-facing Interval B (configurator / presets) wins on spawn & param reload.
    interval_glob.set(interval_b.min(INTERVAL_MAX_DEGREES as usize) as u8);

    // Initial button LEDs
    leds.set(
        0,
        Led::Button,
        Logic::from_u8(glob_logic.get()).color(),
        LED_BRIGHTNESS,
    );
    if glob_muted.get() {
        leds.unset(1, Led::Button);
    } else {
        leds.set(1, Led::Button, color_a, LED_BRIGHTNESS);
    }

    let clock_drain = async {
        let mut clock = app.use_clock();
        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Tick(tick) => {
                    glob_ticks.set(tick);
                }
                ClockEvent::Reset | ClockEvent::Stop => {
                    glob_clock_halt.set(true);
                }
                _ => {}
            }
        }
    };

    let clock_processor = async {
        let mut note_on_a = false;
        let mut note_on_b = false;
        let mut sounding_a = note_a;
        let mut sounding_b = note_a + MidiNote::from(1);
        // Reading the device tonality copies the whole GlobalConfig, which is
        // far too heavy for the step path. Refresh at the start of each
        // A-cycle — a pattern boundary is also where a key change belongs
        // musically. Asking for scale and tonic together costs one copy, not
        // two; the local key is irrelevant because Venn always follows the
        // device scale.
        let resolve = || self::follow_key::root_and_key(follow_tonic, true, note_a, Key::Chromatic);
        let (root0, key0) = resolve();
        let mut cached_root = MidiNote::from(root0);
        let mut cached_pcs = scale_pcs(key0);
        let mut last_cycle = u32::MAX;
        // Step and gate-off used to test the tick number for exact equality.
        // The gatekeeper publishes ticks immediately, so a subscriber that
        // falls behind drops them instead of stalling the device clock — and a
        // missed gate-off tick left the note and the CV gate stuck high until
        // the next one happened to land. Track the step instead.
        let mut last_step: Option<u32> = None;
        let mut gate_off_done = true;
        let mut last_processed_tick: u64 = 0;

        loop {
            app.delay_millis(1).await;

            if glob_clock_halt.get() {
                glob_clock_halt.set(false);
                last_step = None;
                gate_off_done = true;
                last_processed_tick = glob_ticks.get();
                midi.send_note_off(sounding_a).await;
                midi.send_note_off(sounding_b).await;
                note_on_a = false;
                note_on_b = false;
                jack[0].set_low().await;
                jack[1].set_low().await;
                leds.unset(0, Led::Top);
                leds.unset(1, Led::Top);
                leds.unset(0, Led::Bottom);
                leds.unset(1, Led::Bottom);
                continue;
            }

            let tick = glob_ticks.get();
            if tick == last_processed_tick {
                continue;
            }
            last_processed_tick = tick;
            let clkn = tick as u32;
            let muted = glob_muted.get();
            let latch = glob_latch_layer.get();
            let logic = Logic::from_u8(glob_logic.get());
            let einv = glob_einv.get();

            let len_a = len_a_glob.get().max(2);
            let len_b = len_b_glob.get().max(2);
            let pulses_a = pulses_a_glob.get().min(len_a);
            let pulses_b = pulses_b_glob.get().min(len_b);
            let extent = extent_glob.get();
            let interval = interval_glob.get();

            let step_now = clkn / div;
            if last_step != Some(step_now) {
                    last_step = Some(step_now);
                    gate_off_done = false;
                    let step = step_now;
                    // Rotation fixed at 0 — Alt latches are Extent / Interval.
                    let a = euclidean_at(len_a, pulses_a, 0, step);
                    let b = euclidean_at(len_b, pulses_b, 0, step);

                    // Ch2 stays Layer B so the logic result on Ch1 is the only
                    // thing the mode changes. Deriving Ch2 from the logic too
                    // made OR/AND/XOR emit the same onset set (`a || b`), just
                    // swapped between the two voices — inaudible over MIDI.
                    let (mut out0, mut out1) = match logic {
                        Logic::Or => (a || b, b),
                        Logic::And => (a && b, b),
                        Logic::Xor => (a ^ b, b),
                        Logic::Accnt => (a, a && b),
                    };
                    if einv {
                        out0 = !out0;
                        out1 = !out1;
                    }

                    // Optional Prob % (100 = deterministic)
                    if prob < 100 {
                        let thr = (prob.clamp(0, 100) as u32 * 4095 / 100) as u16;
                        if out0 && die.roll() > thr {
                            out0 = false;
                        }
                        if out1 && die.roll() > thr {
                            out1 = false;
                        }
                    }

                    // Scale-degree arch from Length-A; Ch2 = line + diatonic Interval.
                    let cycle = step / u32::from(len_a);
                    if cycle != last_cycle {
                        last_cycle = cycle;
                        let (r, k) = resolve();
                        cached_root = MidiNote::from(r);
                        cached_pcs = scale_pcs(k);
                    }
                    let deg = u16::from(melody_degree(step, len_a, extent));
                    let line = midi_at_degree(cached_root, &cached_pcs, deg);
                    let line_b =
                        midi_at_degree(cached_root, &cached_pcs, deg + u16::from(interval));

                    if !muted {
                        if out0 {
                            let vel_a = midi_velocity(4095, vel, die.roll());
                            midi.send_note_on(line, vel_a).await;
                            sounding_a = line;
                            sounding_a_glob.set(line);
                            jack[0].set_high().await;
                            note_on_a = true;
                        }
                        if out1 {
                            let vel_b = midi_velocity(4095, vel, die.roll());
                            midi.send_note_on(line_b, vel_b).await;
                            sounding_b = line_b;
                            sounding_b_glob.set(line_b);
                            jack[1].set_high().await;
                            note_on_b = true;
                        }
                    }

                    // Layer hits: soft dim on buttons (62% remain); Top/Bottom keep Flash.
                    if a {
                        glob_btn_a_flash.set(BUTTON_HIT_FLASH_MS);
                    }
                    if b && !muted {
                        glob_btn_b_flash.set(BUTTON_HIT_FLASH_MS);
                    }
                    if latch == LatchLayer::Main {
                        if a {
                            leds.set_mode(0, Led::Top, LedMode::Flash(color_a, Some(1)));
                        }
                        if b {
                            leds.set_mode(1, Led::Top, LedMode::Flash(color_b, Some(1)));
                        }
                        if out0 && !muted {
                            leds.set_mode(
                                0,
                                Led::Bottom,
                                LedMode::Flash(logic.color(), Some(1)),
                            );
                        }
                        if out1 && !muted {
                            leds.set_mode(
                                1,
                                Led::Bottom,
                                LedMode::Flash(logic.color(), Some(1)),
                            );
                        }
                    }
                    }

            // Gate off
            let gate_off = (div * gatel as u32 / 100).clamp(1, div.saturating_sub(1));
            if !gate_off_done && clkn % div >= gate_off {
                    gate_off_done = true;
                    if note_on_a {
                        midi.send_note_off(sounding_a).await;
                        note_on_a = false;
                        jack[0].set_low().await;
                    }
                    if note_on_b {
                        midi.send_note_off(sounding_b).await;
                        note_on_b = false;
                        jack[1].set_low().await;
                    }
                    }
            // Alt/Third fader meters paint in the 1 ms loop (not clock-bound).
        }
    };

    let fut_clock = async {
        join(clock_drain, clock_processor).await;
    };

    let fut_buttons = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_down().await;
            if shift {
                if chan == 0 {
                    // Cycle Logic
                    let next = Logic::from_u8(glob_logic.get()).next();
                    glob_logic.set(next as u8);
                    storage.modify_and_save(|s| {
                        s.logic = next as u8;
                    });
                    leds.set_mode(0, Led::Button, LedMode::Flash(next.color(), Some(2)));
                    glob_btn_flash.set(BUTTON_FLASH_MS);
                } else if chan == 1 {
                    // Toggle eInv — white↔none fade like Heat Pump invert.
                    let einv = !glob_einv.get();
                    glob_einv.set(einv);
                    storage.modify_and_save(|s| {
                        s.einv = einv;
                    });
                    // Invert on → white→none; invert off → none→white.
                    glob_btn_b_flash.set(0);
                    glob_einv_fade_up.set(!einv);
                    glob_einv_fade.set(EINVERT_FADE_MS);
                }
            } else if chan == 1 {
                // Btn2: hold+fader = Third (length); tap (no fader move) = mute.
                // Mute must not fire on down — that made hold look like a dead/black button.
                glob_fader_moved.set(false);
                let start0 = faders.get_value_at(0);
                let start1 = faders.get_value_at(1);
                loop {
                    match select(buttons.wait_for_up(1), faders.wait_for_any_change()).await {
                        Either::First(_) => break,
                        Either::Second(_) => {
                            if faders.get_value_at(0).abs_diff(start0) > FADER_MOVE_THRESH
                                || faders.get_value_at(1).abs_diff(start1) > FADER_MOVE_THRESH
                            {
                                glob_fader_moved.set(true);
                            }
                        }
                    }
                }
                if !glob_fader_moved.get() {
                    let muted = glob_muted.toggle();
                    storage.modify_and_save(|s| {
                        s.muted = muted;
                    });
                    if muted {
                        jack[0].set_low().await;
                        jack[1].set_low().await;
                        midi.send_note_off(sounding_a_glob.get()).await;
                        midi.send_note_off(sounding_b_glob.get()).await;
                        leds.unset(1, Led::Button);
                    } else {
                        leds.set(1, Led::Button, color_a, LED_BRIGHTNESS);
                    }
                }
            }
            // Btn0/Btn1 hold = Third layer via shift task (length A/B)
        }
    };

    let fut_latch = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];
        loop {
            let chan = faders.wait_for_any_change().await;
            let layer = glob_latch_layer.get();
            let target = match (chan, layer) {
                (0, LatchLayer::Main) => storage.query(|s| s.pulses_a),
                (0, LatchLayer::Alt) => storage.query(|s| s.extent),
                (0, LatchLayer::Third) => storage.query(|s| s.len_a),
                (1, LatchLayer::Main) => storage.query(|s| s.pulses_b),
                (1, LatchLayer::Alt) => storage.query(|s| s.interval),
                (1, LatchLayer::Third) => storage.query(|s| s.len_b),
                _ => 0,
            };
            if let Some(new_value) =
                latch[chan].update(faders.get_value_at(chan), layer, target)
            {
                match (chan, layer) {
                    (0, LatchLayer::Main) => {
                        let len = len_a_glob.get();
                        pulses_a_glob.set(pulses_from_fader(new_value, len));
                        storage.modify_and_save(|s| s.pulses_a = new_value);
                    }
                    (0, LatchLayer::Alt) => {
                        extent_glob.set(extent_from_fader(new_value));
                        storage.modify_and_save(|s| s.extent = new_value);
                    }
                    (0, LatchLayer::Third) => {
                        let len = length_from_fader(new_value);
                        len_a_glob.set(len);
                        // Re-clamp pulses against new length
                        let pa = storage.query(|s| s.pulses_a);
                        pulses_a_glob.set(pulses_from_fader(pa, len));
                        storage.modify_and_save(|s| s.len_a = new_value);
                    }
                    (1, LatchLayer::Main) => {
                        let len = len_b_glob.get();
                        pulses_b_glob.set(pulses_from_fader(new_value, len));
                        storage.modify_and_save(|s| s.pulses_b = new_value);
                    }
                    (1, LatchLayer::Alt) => {
                        interval_glob.set(interval_from_fader(new_value));
                        storage.modify_and_save(|s| s.interval = new_value);
                    }
                    (1, LatchLayer::Third) => {
                        let len = length_from_fader(new_value);
                        len_b_glob.set(len);
                        let pb = storage.query(|s| s.pulses_b);
                        pulses_b_glob.set(pulses_from_fader(pb, len));
                        storage.modify_and_save(|s| s.len_b = new_value);
                    }
                    _ => {}
                }
            }
        }
    };

    let fut_scene = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (pa, pb, ext, iv, la, lb, logic, einv, muted) = storage.query(|s| {
                        (
                            s.pulses_a,
                            s.pulses_b,
                            s.extent,
                            s.interval,
                            s.len_a,
                            s.len_b,
                            s.logic,
                            s.einv,
                            s.muted,
                        )
                    });
                    let len_a = length_from_fader(la);
                    let len_b = length_from_fader(lb);
                    len_a_glob.set(len_a);
                    len_b_glob.set(len_b);
                    pulses_a_glob.set(pulses_from_fader(pa, len_a));
                    pulses_b_glob.set(pulses_from_fader(pb, len_b));
                    extent_glob.set(extent_from_fader(ext));
                    interval_glob.set(interval_from_fader(iv));
                    glob_logic.set(Logic::from_u8(logic) as u8);
                    glob_einv.set(einv);
                    glob_muted.set(muted);

                    leds.set(
                        0,
                        Led::Button,
                        Logic::from_u8(logic).color(),
                        LED_BRIGHTNESS,
                    );
                    if muted {
                        leds.unset(1, Led::Button);
                    } else {
                        leds.set(1, Led::Button, color_a, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let fut_shift = async {
        let mut prev_latch = LatchLayer::Main;
        loop {
            app.delay_millis(1).await;

            // Third = hold Btn1 or Btn2 (length); Alt = Shift without channel button.
            let latch_active = if buttons.is_shift_pressed()
                && !buttons.is_button_pressed(0)
                && !buttons.is_button_pressed(1)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed()
                && (buttons.is_button_pressed(0) || buttons.is_button_pressed(1))
            {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active);

            // Leaving meters: Main clears all; Alt clears Bottom (Third bands).
            if prev_latch != latch_active {
                match latch_active {
                    LatchLayer::Main => {
                        leds.unset(0, Led::Top);
                        leds.unset(1, Led::Top);
                        leds.unset(0, Led::Bottom);
                        leds.unset(1, Led::Bottom);
                    }
                    LatchLayer::Alt => {
                        leds.unset(0, Led::Bottom);
                        leds.unset(1, Led::Bottom);
                    }
                    LatchLayer::Third => {}
                }
            }
            prev_latch = latch_active;

            // Button LED paint: density base; soft hit dip; skip while logic Flash plays.
            let flash_left = glob_btn_flash.get();
            if flash_left > 0 {
                glob_btn_flash.set(flash_left.saturating_sub(1));
            }
            // Decrement after sampling so this frame still paints the dip.
            let hit_a = glob_btn_a_flash.get();
            let hit_b = glob_btn_b_flash.get();
            if hit_a > 0 {
                glob_btn_a_flash.set(hit_a.saturating_sub(1));
            }
            if hit_b > 0 {
                glob_btn_b_flash.set(hit_b.saturating_sub(1));
            }

            // eInv feedback (white↔off) on Btn2 — mirrors Heat Pump invert.
            let einv_fade = glob_einv_fade.get();
            if einv_fade > 0 {
                let elapsed = EINVERT_FADE_MS.saturating_sub(einv_fade);
                let bright = if glob_einv_fade_up.get() {
                    ((elapsed as u32 * 255) / EINVERT_FADE_MS as u32) as u8
                } else {
                    (((EINVERT_FADE_MS - elapsed) as u32 * 255) / EINVERT_FADE_MS as u32) as u8
                };
                leds.set(1, Led::Button, Color::White, Brightness::Custom(bright));
                let next = einv_fade.saturating_sub(1);
                glob_einv_fade.set(next);
                if next == 0 {
                    if glob_muted.get() {
                        leds.unset(1, Led::Button);
                    } else {
                        let len_b = len_b_glob.get().max(1);
                        let base_b = button_density_bright(pulses_b_glob.get(), len_b);
                        leds.set(1, Led::Button, color_a, Brightness::Custom(base_b));
                    }
                }
            }

            let len_a = len_a_glob.get().max(1);
            let len_b = len_b_glob.get().max(1);
            let muted = glob_muted.get();

            match latch_active {
                // Hit dips always paint on Main (even during logic Flash hold-off on the other path).
                LatchLayer::Main => {
                    if flash_left > 0 {
                        // Logic Flash owns Btn1; still allow Btn2 hit dips.
                        if muted {
                            if einv_fade == 0 {
                                leds.unset(1, Led::Button);
                            }
                        } else if einv_fade == 0 {
                            let base_b = button_density_bright(pulses_b_glob.get(), len_b);
                            let bright_b = if hit_b > 0 {
                                hit_dip_bright(base_b)
                            } else {
                                base_b
                            };
                            leds.set(1, Led::Button, color_a, Brightness::Custom(bright_b));
                        }
                    } else {
                        let base_a = button_density_bright(pulses_a_glob.get(), len_a);
                        let base_b = button_density_bright(pulses_b_glob.get(), len_b);
                        let bright_a = if hit_a > 0 {
                            hit_dip_bright(base_a)
                        } else {
                            base_a
                        };
                        leds.set(
                            0,
                            Led::Button,
                            Logic::from_u8(glob_logic.get()).color(),
                            Brightness::Custom(bright_a),
                        );
                        if muted {
                            if einv_fade == 0 {
                                leds.unset(1, Led::Button);
                            }
                        } else if einv_fade == 0 {
                            let bright_b = if hit_b > 0 {
                                hit_dip_bright(base_b)
                            } else {
                                base_b
                            };
                            leds.set(1, Led::Button, color_a, Brightness::Custom(bright_b));
                        }
                    }
                }
                LatchLayer::Alt if flash_left == 0 => {
                    // Extent / Interval meters: Top + buttons.
                    let ba = extent_meter_bright(extent_glob.get());
                    let bb = interval_meter_bright(interval_glob.get());
                    leds.set(0, Led::Top, color_a, Brightness::Custom(ba));
                    leds.set(1, Led::Top, color_b, Brightness::Custom(bb));
                    leds.set(
                        0,
                        Led::Button,
                        Logic::from_u8(glob_logic.get()).color(),
                        Brightness::Custom(ba),
                    );
                    if muted {
                        if einv_fade == 0 {
                            leds.unset(1, Led::Button);
                        }
                    } else if einv_fade == 0 {
                        leds.set(1, Led::Button, color_a, Brightness::Custom(bb));
                    }
                }
                LatchLayer::Third if flash_left == 0 => {
                    // Length: shortest hue from each btn base → its complement; bright ∝ len.
                    let logic_c = Logic::from_u8(glob_logic.get()).color();
                    let ca = length_grad_color(len_a, logic_c, complement_color(logic_c));
                    let cb = length_grad_color(len_b, color_a, complement_color(color_a));
                    let ba = len_meter_bright(len_a);
                    let bb = len_meter_bright(len_b);
                    leds.set(0, Led::Top, ca, Brightness::Custom(ba));
                    leds.set(1, Led::Top, cb, Brightness::Custom(bb));
                    leds.set(0, Led::Bottom, ca, Brightness::Mid);
                    leds.set(1, Led::Bottom, cb, Brightness::Mid);
                    leds.set(0, Led::Button, ca, Brightness::Custom(ba));
                    if muted {
                        if einv_fade == 0 {
                            leds.unset(1, Led::Button);
                        }
                    } else if einv_fade == 0 {
                        leds.set(1, Led::Button, cb, Brightness::Custom(bb));
                    }
                }
                _ => {}
            }
        }
    };

    join5(fut_clock, fut_buttons, fut_latch, fut_scene, fut_shift).await;
}

mod follow_key {
    use libfp::{Key, MidiNote};
    use midly::num::u7;

    use crate::tasks::global_config::get_global_config;

    pub fn root_and_key(
        follow_tonic: bool,
        follow_scale: bool,
        local_root: MidiNote,
        local_key: Key,
    ) -> (u8, Key) {
        if !follow_tonic && !follow_scale {
            return (midi_u8(local_root), local_key);
        }
        let c = get_global_config();
        let pc = if follow_tonic {
            c.quantizer.tonic as u8
        } else {
            midi_u8(local_root) % 12
        };
        let key = if follow_scale {
            match c.quantizer.key {
                Key::Off => Key::Chromatic,
                k => k,
            }
        } else {
            local_key
        };
        (retune(local_root, pc), key)
    }

    fn retune(root: MidiNote, pc: u8) -> u8 {
        let n = midi_u8(root);
        let target = (n - n % 12) + pc % 12;
        if target > 127 {
            target - 12
        } else {
            target
        }
    }

    fn midi_u8(note: MidiNote) -> u8 {
        u7::from(note).as_int()
    }
}

mod led_fx {
    pub fn hsv_to_rgb(hue: u16) -> (u8, u8, u8) {
        let hue = hue % 360;
        let sector = hue / 60;
        let ramp = ((hue % 60) as u32 * 255 / 59) as u8;
        match sector {
            0 => (255, ramp, 0),
            1 => (255 - ramp, 255, 0),
            2 => (0, 255, ramp),
            3 => (0, 255 - ramp, 255),
            4 => (ramp, 0, 255),
            _ => (255, 0, 255 - ramp),
        }
    }
}

