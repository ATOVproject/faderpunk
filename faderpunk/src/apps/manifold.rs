use embassy_futures::{
    join::{join, join3, join4},
    select::{select},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{
        attenuverter, clickless, midi_gate, slew_lin, split_unsigned_value, SlewState,
    },
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiNote,
    MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::{
    app::{
        App, AppParams, AppStorage, ClockEvent, GateJack, Led, Leds, ManagedStorage, MidiOutput,
        OutJack, SceneEvent,
    },
    tasks::leds::LedMode,
};

use self::morph::{morph_sample, MorphChaos};

pub const CHANNELS: usize = 4;
pub const PARAMS: usize = 16;

/// Mirrored waves: the conditioned input / internal LFO on Ch0, then Out B, C
/// and D. Both maps below pack one field per wave in that order.
const MIDI_WAVES: usize = 4;

/// `Ch Map` packs one channel per wave into a nibble (wave 0 = bits 0..3).
/// A whole map of 0 means "every wave follows the base MIDI Channel", which is
/// the shipped default; any non-zero map is read literally, nibble + 1 = channel.
const CH_MAP_FOLLOW: i32 = 0;
const CH_MAP_MAX: i32 = (1 << (4 * MIDI_WAVES)) - 1;
/// `CC Map` packs one CC number per wave into 7 bits (wave 0 = bits 0..6).
/// A whole map of 0 keeps the historic base + wave index derivation.
const CC_MAP_FOLLOW: i32 = 0;
const CC_MAP_MAX: i32 = (1 << (7 * MIDI_WAVES)) - 1;
/// `Mode Map` packs one Note/CC flag per wave (wave 0 = bit 0). 0 = every
/// wave sends CC (legacy); a set bit with Gate/Trigger mode sends Note On/Off.
const MODE_MAP_FOLLOW: i32 = 0;
const MODE_MAP_MAX: i32 = (1 << MIDI_WAVES) - 1;
/// Keeps the literal bounds in `CONFIG` honest against the packing above.
const _: () = assert!(CH_MAP_FOLLOW == 0 && CH_MAP_MAX == 65_535);
const _: () = assert!(CC_MAP_FOLLOW == 0 && CC_MAP_MAX == 268_435_455);
const _: () = assert!(MODE_MAP_FOLLOW == 0 && MODE_MAP_MAX == 15);

const TRIG_HIGH: u16 = 2458;

fn hsv_to_rgb(hue: u16) -> (u8, u8, u8) {
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

fn signal_brightness(value: u16, bipolar: bool) -> Brightness {
    const MIN: u8 = 110;
    let raw = if bipolar {
        ((value.min(4095) as i32 - 2047).unsigned_abs() / 8).min(255) as u8
    } else {
        (value.min(4095) / 16) as u8
    };
    let span = 255u16 - MIN as u16;
    Brightness::Custom(MIN + ((raw as u16 * span) / 255) as u8)
}

const FADER_MOVE_THRESH: u16 = 64;
const BUTTON_BRIGHTNESS: Brightness = Brightness::Mid;
/// Idle button presence so the four channels stay identifiable as Manifold.
const BUTTON_IDLE_BRIGHTNESS: Brightness = Brightness::Low;
/// Input samples within this 12-bit distance count as unchanged (ADC noise floor).
const IN_DEADBAND: u16 = 24;
/// Milliseconds of unchanged input before the app falls back to the internal
/// LFO. Long enough that a slow CV ramp is never mistaken for silence.
const IN_IDLE_MS: u16 = 1200;
/// Hold off periodic button LED writes so LedMode::Flash can finish.
const BUTTON_FLASH_MS: u16 = 850;
/// One `LedMode::Flash` cycle ≈ 16 frames at 60 Hz.
const RANGE_FLASH_CYCLE_MS: u16 = 270;
/// Internal LFO shape defaults for the axes Manifold does not expose:
/// balanced halves, no time warp. Skew stays on the Ch0 Third layer.
const LFO_SYMMETRY: u16 = 2048;
const LFO_WARP: u16 = 0;
/// Clock divisions the speed fader spans (slowest 384 ticks … 6 = quarter note).
const LFO_DIVISIONS: usize = 9;
const LFO_CLOCK_TABLE: [u32; LFO_DIVISIONS] = [384, 192, 96, 48, 24, 16, 12, 8, 6];

fn division_at(fader: u16, count: usize) -> u32 {
    let count = count.clamp(1, LFO_CLOCK_TABLE.len());
    let idx = (fader.min(4095) as usize * count / 4096).min(count - 1);
    LFO_CLOCK_TABLE[idx]
}

fn lfo_step(speed: u16) -> f32 {
    (Curve::Exponential.at(speed) as f32) * 0.015 + 0.0682
}

/// Five mode hues. Ch0 CV (200) and Ch0 LFO (238) are shared with Ripppple;
/// out modes CV / Gate / Trigger fan +38 deg: 276 / 314 / 352. Steady LEDs
/// follow the active mode via glob_modes.
const MANIFOLD_HUES: [u16; 5] = [200, 238, 276, 314, 352];

fn manifold_color(step: usize) -> Color {
    let (r, g, b) = hsv_to_rgb(MANIFOLD_HUES[step.min(4)] % 360);
    Color::Custom(r, g, b)
}

fn ch0_color(lfo_active: bool) -> Color {
    if lfo_active {
        manifold_color(1)
    } else {
        manifold_color(0)
    }
}

const fn pulse_ms_to_fader(ms: u16) -> u16 {
    let ms = if ms < 1 {
        1
    } else if ms > 100 {
        100
    } else {
        ms
    };
    ((ms - 1) as u32 * 4095 / 99) as u16
}

const fn pulse_fader_to_ms(value: u16) -> u16 {
    1 + (value as u32 * 99 / 4095) as u16
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Cv,
    Gate,
    Trigger,
}

impl Mode {
    fn from_usize(v: usize) -> Self {
        match v {
            1 => Mode::Gate,
            2 => Mode::Trigger,
            _ => Mode::Cv,
        }
    }

    fn next(self) -> Self {
        match self {
            Mode::Cv => Mode::Gate,
            Mode::Gate => Mode::Trigger,
            Mode::Trigger => Mode::Cv,
        }
    }

    /// Button / meter hue for this out mode — persistent, not channel-based.
    fn color(self) -> Color {
        match self {
            Mode::Cv => manifold_color(2),
            Mode::Gate => manifold_color(3),
            Mode::Trigger => manifold_color(4),
        }
    }
}

fn next_range(range: Range) -> Range {
    match range {
        Range::_0_10V => Range::_Neg5_5V,
        _ => Range::_0_10V,
    }
}

/// ±5V → one blink; 0–10V → two blinks.
fn range_flash_times(range: Range) -> usize {
    if range.is_bipolar() {
        1
    } else {
        2
    }
}

fn range_flash_hold_ms(times: usize) -> u16 {
    RANGE_FLASH_CYCLE_MS
        .saturating_mul(times as u16)
        .saturating_add(40)
}

enum OutJackType {
    Cv(OutJack),
    Gate(GateJack),
}

fn paint_bipolar_level(leds: &Leds<CHANNELS>, chan: usize, color: Color, level: u16) {
    let parts = split_unsigned_value(level);
    leds.set(chan, Led::Top, color, Brightness::Custom(parts[0]));
    leds.set(chan, Led::Bottom, color, Brightness::Custom(parts[1]));
}

fn paint_buttons(
    leds: &Leds<CHANNELS>,
    in_color: Color,
    modes: [Mode; 3],
    frozen: bool,
    muted: [bool; 3],
) {
    leds.set(
        0,
        Led::Button,
        in_color,
        if frozen {
            BUTTON_BRIGHTNESS
        } else {
            BUTTON_IDLE_BRIGHTNESS
        },
    );
    for (i, &muted_i) in muted.iter().enumerate() {
        if muted_i {
            leds.unset(i + 1, Led::Button);
        } else {
            leds.set(i + 1, Led::Button, modes[i].color(), BUTTON_BRIGHTNESS);
        }
    }
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Manifold",
    "One CV in, three shaped outs with comparator modes",
    Color::Violet,
    AppIcon::Attenuate,
)
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
.add_param(Param::Range {
    name: "In Range",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Mode B",
    variants: &["CV", "Gate", "Trigger"],
})
.add_param(Param::Range {
    name: "Range B",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Mode C",
    variants: &["CV", "Gate", "Trigger"],
})
.add_param(Param::Range {
    name: "Range C",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Mode D",
    variants: &["CV", "Gate", "Trigger"],
})
.add_param(Param::Range {
    name: "Range D",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "LFO Speed",
    variants: &["Normal", "Slow", "Slowest"],
})
.add_param(Param::MidiOut)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
.add_param(Param::MidiNrpn)
// Literal bounds: the catalog generator reads these as syntax, and a path
// expression would come out as an enum tag instead of a number.
.add_param(Param::i32 {
    name: "Ch Map",
    min: 0,
    max: 65_535,
})
.add_param(Param::i32 {
    name: "CC Map",
    min: 0,
    max: 268_435_455,
})
.add_param(Param::i32 {
    name: "Mode Map",
    min: 0,
    max: 15,
});

pub(crate) struct Params {
    pub(crate) color: Color,
    pub(crate) in_range: Range,
    pub(crate) mode_b: Mode,
    pub(crate) range_b: Range,
    pub(crate) mode_c: Mode,
    pub(crate) range_c: Range,
    pub(crate) mode_d: Mode,
    pub(crate) range_d: Range,
    pub(crate) lfo_speed_mult: usize,
    pub(crate) midi_out: MidiOut,
    pub(crate) midi_channel: MidiChannel,
    /// Base CC; the four channels take base + 0..=3.
    pub(crate) midi_cc: MidiCc,
    pub(crate) nrpn: bool,
    /// Per-wave MIDI channel, 4 bit each; 0 = every wave follows `midi_channel`.
    pub(crate) ch_map: i32,
    /// Per-wave CC number, 7 bit each; 0 = base CC + wave index.
    pub(crate) cc_map: i32,
    /// Per-wave Note vs CC, 1 bit each; 0 = CC for every wave (legacy).
    pub(crate) mode_map: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            color: Color::Violet,
            in_range: Range::_Neg5_5V,
            mode_b: Mode::Cv,
            range_b: Range::_Neg5_5V,
            mode_c: Mode::Gate,
            range_c: Range::_Neg5_5V,
            mode_d: Mode::Trigger,
            range_d: Range::_Neg5_5V,
            lfo_speed_mult: 0,
            midi_out: MidiOut([false; 3]),
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8),
            nrpn: false,
            ch_map: CH_MAP_FOLLOW,
            cc_map: CC_MAP_FOLLOW,
            mode_map: MODE_MAP_FOLLOW,
        }
    }
}

impl Params {
    /// Resolve the sending channel for one mirrored wave.
    fn channel_for(&self, wave: usize) -> MidiChannel {
        if self.ch_map == CH_MAP_FOLLOW {
            return self.midi_channel;
        }
        let nibble = ((self.ch_map >> (4 * wave)) & 0xF) as u8;
        MidiChannel::from(nibble + 1)
    }

    /// Resolve the CC number for one mirrored wave.
    fn cc_for(&self, wave: usize, nrpn: bool) -> MidiCc {
        if self.cc_map == CC_MAP_FOLLOW {
            return channel_cc(self.midi_cc, wave, nrpn);
        }
        let field = ((self.cc_map >> (7 * wave)) & 0x7F) as u16;
        MidiCc::from(field.min(midi_cc_limit(nrpn)))
    }

    /// True when this wave sends Note On/Off instead of CC.
    fn midi_is_note(&self, wave: usize, modes: &[Mode; 3]) -> bool {
        if wave == 0 || self.mode_map == MODE_MAP_FOLLOW {
            return false;
        }
        if ((self.mode_map >> wave) & 1) == 0 {
            return false;
        }
        matches!(modes[wave - 1], Mode::Gate | Mode::Trigger)
    }

    /// Note number for one wave, taken from that wave's CC Map slot.
    fn note_for(&self, wave: usize) -> MidiNote {
        let pitch = if self.cc_map == CC_MAP_FOLLOW {
            channel_cc(self.midi_cc, wave, false).as_u16()
        } else {
            ((self.cc_map >> (7 * wave)) & 0x7F) as u16
        };
        // Notes are always 7-bit, even when the CC path is in NRPN mode.
        MidiNote::from(pitch.min(127) as u8)
    }
}

/// Highest CC number the transport can carry: NRPN uses the full 14-bit
/// parameter space, plain CC only 7 bit (`MidiCc -> u7` truncates silently,
/// so a wrapped number would land on an unrelated controller).
const fn midi_cc_limit(nrpn: bool) -> u16 {
    if nrpn {
        16383
    } else {
        127
    }
}

/// Base CC offset by the channel index, saturated at the transport limit.
/// Saturating rather than wrapping: at the very top of the range two channels
/// collide on one CC, which is obvious and fixable by lowering the base —
/// wrapping would instead hijack CC 0..2 with no visible cause.
fn channel_cc(base: MidiCc, chan: usize, nrpn: bool) -> MidiCc {
    MidiCc::from(
        base.as_u16()
            .saturating_add(chan as u16)
            .min(midi_cc_limit(nrpn)),
    )
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Legacy: 8 params (no internal LFO), 10 (no MIDI mirror), 14 = MIDI,
        // 17 = old layout with Source at index 8. New format: 16 params, no Source.
        if values.len() < 8 {
            return None;
        }
        let source_skip = if values.len() >= 17 {
            1
        } else if values.len() > 9 {
            match values.get(9) {
                Some(Value::MidiOut(_)) => 0,
                Some(Value::Enum(_)) => 1,
                _ => 0,
            }
        } else {
            0
        };
        let lfo_idx = 8 + source_skip;
        let midi_out_idx = lfo_idx + 1;
        let (midi_out, midi_channel, midi_cc, nrpn) = if values.len() >= midi_out_idx + 4 {
            (
                MidiOut::from_value(values[midi_out_idx]),
                MidiChannel::from_value(values[midi_out_idx + 1]),
                MidiCc::from_value(values[midi_out_idx + 2]),
                bool::from_value(values[midi_out_idx + 3]),
            )
        } else {
            // Presets saved before the mirror existed must stay silent on MIDI.
            (
                MidiOut([false; 3]),
                MidiChannel::default(),
                MidiCc::from(32u8),
                false,
            )
        };
        let ch_map_idx = midi_out_idx + 4;
        let cc_map_idx = midi_out_idx + 5;
        let mode_map_idx = midi_out_idx + 6;
        Some(Self {
            color: Color::from_value(values[0]),
            in_range: Range::from_value(values[1]),
            mode_b: Mode::from_usize(usize::from_value(values[2])),
            range_b: Range::from_value(values[3]),
            mode_c: Mode::from_usize(usize::from_value(values[4])),
            range_c: Range::from_value(values[5]),
            mode_d: Mode::from_usize(usize::from_value(values[6])),
            range_d: Range::from_value(values[7]),
            lfo_speed_mult: if values.len() > lfo_idx {
                usize::from_value(values[lfo_idx])
            } else {
                0
            },
            midi_out,
            midi_channel,
            midi_cc,
            nrpn,
            ch_map: if values.len() > ch_map_idx {
                i32::from_value(values[ch_map_idx]).clamp(CH_MAP_FOLLOW, CH_MAP_MAX)
            } else {
                CH_MAP_FOLLOW
            },
            cc_map: if values.len() > cc_map_idx {
                i32::from_value(values[cc_map_idx]).clamp(CC_MAP_FOLLOW, CC_MAP_MAX)
            } else {
                CC_MAP_FOLLOW
            },
            mode_map: if values.len() > mode_map_idx {
                i32::from_value(values[mode_map_idx]).clamp(MODE_MAP_FOLLOW, MODE_MAP_MAX)
            } else {
                MODE_MAP_FOLLOW
            },
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.color.into()).unwrap();
        vec.push(self.in_range.into()).unwrap();
        vec.push(Value::Enum(self.mode_b as usize)).unwrap();
        vec.push(self.range_b.into()).unwrap();
        vec.push(Value::Enum(self.mode_c as usize)).unwrap();
        vec.push(self.range_c.into()).unwrap();
        vec.push(Value::Enum(self.mode_d as usize)).unwrap();
        vec.push(self.range_d.into()).unwrap();
        vec.push(Value::Enum(self.lfo_speed_mult)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.ch_map.into()).unwrap();
        vec.push(self.cc_map.into()).unwrap();
        vec.push(self.mode_map.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    in_trim: u16,
    in_offset: u16,
    in_slew: u16,
    att: [u16; 3],
    offset: [u16; 3],
    slew: [u16; 3],
    threshold: [u16; 3],
    hysteresis: [u16; 3],
    pulse: [u16; 3],
    muted: [bool; 3],
    /// Internal-LFO layers on Ch0 (Main / Alt / Third) plus its clock sync.
    morph: u16,
    lfo_speed: u16,
    skew: u16,
    lfo_clocked: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            in_trim: 4095,
            in_offset: 2047,
            in_slew: 0,
            att: [4095; 3],
            offset: [2047; 3],
            slew: [0; 3],
            threshold: [TRIG_HIGH; 3],
            hysteresis: [100; 3],
            pulse: [pulse_ms_to_fader(10); 3],
            muted: [false; 3],
            morph: 0,
            lfo_speed: 2000,
            skew: 2048,
            lfo_clocked: false,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let mut params = Params {
        midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
        ..Params::default()
    };
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    storage.load().await;

    let app_loop = async {
        loop {
            select(
                run(&app, &mut params, &storage),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

async fn make_jack_for_mode(
    app: &App<CHANNELS>,
    chan: usize,
    mode: Mode,
    range: Range,
) -> OutJackType {
    match mode {
        Mode::Cv => OutJackType::Cv(app.make_out_jack(chan, range).await),
        Mode::Gate | Mode::Trigger => {
            let g = app.make_gate_jack(chan, 4095).await;
            // make_gate_jack drives the port high on configure; force a known-off state.
            g.set_low().await;
            OutJackType::Gate(g)
        }
    }
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &mut Params,
    storage: &ManagedStorage<Storage>,
) {
    let in_range = params.in_range;
    let mode_b = params.mode_b;
    let range_b = params.range_b;
    let mode_c = params.mode_c;
    let range_c = params.range_c;
    let mode_d = params.mode_d;
    let range_d = params.range_d;
    let nrpn = params.nrpn;
    let modes = [mode_b, mode_c, mode_d];
    let ranges = [range_b, range_c, range_d];
    let midi_out = params.midi_out;
    let midi_chans = core::array::from_fn::<MidiChannel, MIDI_WAVES, _>(|w| params.channel_for(w));
    let midi_ccs = core::array::from_fn::<MidiCc, MIDI_WAVES, _>(|w| params.cc_for(w, nrpn));
    let midi_is_note =
        core::array::from_fn::<bool, MIDI_WAVES, _>(|w| params.midi_is_note(w, &modes));
    let note_pitches =
        core::array::from_fn::<MidiNote, MIDI_WAVES, _>(|w| params.note_for(w));
    let lfo_speed_mult = 2u32.pow(params.lfo_speed_mult.min(31) as u32);

    // Starts idle so an unpatched Manifold comes up on the internal LFO.
    let initial_lfo_active = true;

    // Ch0 (conditioned input / internal LFO), then Out B, C, D — one handle per
    // wave so each can sit on its own MIDI channel.
    let midi: [MidiOutput; MIDI_WAVES] =
        core::array::from_fn(|w| app.use_midi_output(midi_out, midi_chans[w], nrpn));

    for w in 0..MIDI_WAVES {
        if midi_is_note[w] {
            midi[w].send_note_off(note_pitches[w]).await;
        }
    }

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let glob_in_trim = app.make_global(4095u16);
    let glob_in_offset = app.make_global(2047u16);
    let glob_in_slew = app.make_global(0u16);
    let glob_att = app.make_global([4095u16; 3]);
    let glob_offset = app.make_global([2047u16; 3]);
    let glob_slew = app.make_global([0u16; 3]);
    let glob_threshold = app.make_global([TRIG_HIGH; 3]);
    let glob_hysteresis = app.make_global([100u16; 3]);
    let glob_pulse = app.make_global([pulse_ms_to_fader(10); 3]);
    let glob_muted = app.make_global([false; 3]);
    let glob_modes = app.make_global(modes);
    let glob_frozen = app.make_global(false);

    let glob_fader_at_down = app.make_global([0u16; 4]);
    let glob_fader_moved = app.make_global([false; 4]);
    let glob_long_press = app.make_global([false; 4]);
    // Shift is global, so only the channel touched last shows its Alt hint.
    let glob_shift_focus = app.make_global(0usize);

    // Signalled when an out mode changes: run() has to return so the jack is
    // reconfigured between CV out and gate out.
    let restart = Signal::<NoopRawMutex, ()>::new();

    // Internal LFO: runs whenever the input jack has been static long enough,
    // so an unpatched Manifold still has something to shape.
    let die = app.use_die();
    let mut clock = app.use_clock();
    let glob_ticks = app.make_global(0u64);
    let glob_lfo_active = app.make_global(true);
    let glob_lfo_pos = app.make_global(0.0f32);
    let glob_lfo_step = app.make_global(0.0682f32);
    let glob_div = app.make_global(24u32);
    let glob_chaos = app.make_global(MorphChaos::new());
    let glob_btn_flash = app.make_global([0u16; 4]);

    // Free-run step and clock division for the stored speed fader.
    let time_calc = || {
        let speed = storage.query(|s| s.lfo_speed);
        glob_lfo_step.set(lfo_step(speed));
        glob_div.set(division_at(speed, LFO_DIVISIONS));
    };
    time_calc();

    let (in_trim, in_offset, in_slew, att, offset, slew, threshold, hysteresis, pulse, muted) =
        storage.query(|s| {
            (
                s.in_trim,
                s.in_offset,
                s.in_slew,
                s.att,
                s.offset,
                s.slew,
                s.threshold,
                s.hysteresis,
                s.pulse,
                s.muted,
            )
        });

    glob_in_trim.set(in_trim);
    glob_in_offset.set(in_offset);
    glob_in_slew.set(in_slew);
    glob_att.set(att);
    glob_offset.set(offset);
    glob_slew.set(slew);
    glob_threshold.set(threshold);
    glob_hysteresis.set(hysteresis);
    glob_pulse.set(pulse);
    glob_muted.set(muted);

    let in_jack = app.make_in_jack(0, in_range).await;
    let out_jacks = [
        make_jack_for_mode(app, 1, modes[0], ranges[0]).await,
        make_jack_for_mode(app, 2, modes[1], ranges[1]).await,
        make_jack_for_mode(app, 3, modes[2], ranges[2]).await,
    ];

    paint_buttons(
        &leds,
        ch0_color(initial_lfo_active),
        glob_modes.get(),
        false,
        muted,
    );

    let fut1 = async {
        let mut in_slew_state = SlewState::new();
        let mut out_slew_states = [SlewState::new(); 3];
        let mut gate_high = [false; 3];
        let mut pulse_left = [0u16; 3];
        let mut out_levels = [0u16; 3];
        let mut frozen = false;
        let mut frozen_value = 0u16;
        // One gate state per mirrored channel, so a busy channel cannot
        // suppress the others. u16::MAX is out of `midi_gate`'s range and
        // therefore forces one send per channel on startup.
        let mut last_midi = [u16::MAX; 4];
        let mut note_sounding = [false; 4];
        // This loop runs at 1 kHz and mirrors four channels, where the LFO apps
        // mirror one at 125 Hz — unpaced that is enough CC traffic to wedge a
        // USB host. Offer one channel per 8 ms and rotate, so the bus sees at
        // most 125 messages/s in total and each channel still refreshes at
        // ~31 Hz, well above what the scope can show.
        let mut midi_pace: u8 = 0;
        let mut midi_slot: usize = 0;

        // Seeded from storage: a freshly spawned app sits on its saved values
        // instead of audibly gliding to them from the defaults.
        let mut prev_in_trim = in_trim;
        let mut prev_in_offset = in_offset;
        let mut prev_att = att;
        let mut prev_offset = offset;
        let mut prev_threshold = threshold;
        let mut prev_hysteresis = hysteresis;

        let mut prev_raw_input = in_jack.get_value();
        // Starts idle so an unpatched Manifold comes up on the internal LFO
        // instead of waiting out the timeout on a dead jack.
        let mut in_idle_ms = IN_IDLE_MS;
        // Polled clock state: last seen 24 PPQN counter, milliseconds since it
        // last moved, and the measured tick period for intra-tick interpolation.
        let mut last_tick = glob_ticks.get();
        let mut ms_since_tick = 0u16;
        let mut tick_period_ms = 21u16;

        loop {
            app.delay_millis(1).await;

            let raw_input = in_jack.get_value();

            // Patch heuristic: the jack has no sense pin, so a live cable is
            // inferred from movement. Deadband covers the ADC noise floor.
            if raw_input.abs_diff(prev_raw_input) > IN_DEADBAND {
                in_idle_ms = 0;
            } else {
                in_idle_ms = in_idle_ms.saturating_add(1).min(IN_IDLE_MS);
            }
            prev_raw_input = raw_input;

            let lfo_active = in_idle_ms >= IN_IDLE_MS;
            glob_lfo_active.set(lfo_active);

            let flash = glob_btn_flash.modify(|f| {
                let mut arr = *f;
                for ms in arr.iter_mut() {
                    *ms = ms.saturating_sub(1);
                }
                arr
            });

            let (morph, skew, lfo_clocked) =
                storage.query(|s| (s.morph, s.skew, s.lfo_clocked));

            let tick = glob_ticks.get();
            if tick != last_tick {
                // Plausible tick gaps only: ignore the counter reset to u64::MAX
                // and anything slower than 2 s, which would be a stopped clock.
                if (1..500).contains(&ms_since_tick) && tick > last_tick {
                    tick_period_ms = ms_since_tick;
                }
                last_tick = tick;
                ms_since_tick = 0;
            } else {
                ms_since_tick = ms_since_tick.saturating_add(1);
            }

            let held = glob_frozen.get();

            let source_val = if lfo_active {
                time_calc();

                let next_pos = if lfo_clocked {
                    // Phase locked to the tick counter, interpolated inside the
                    // tick so coarse divisions still move smoothly. A stopped
                    // transport stops the counter, which parks the wave by
                    // itself — no separate hold state.
                    if tick == u64::MAX {
                        0.0
                    } else {
                        let ticks_per_cycle =
                            (glob_div.get() as u64).saturating_mul(lfo_speed_mult as u64).max(1);
                        let frac =
                            (ms_since_tick as f32 / tick_period_ms.max(1) as f32).min(1.0);
                        let pos_ticks = (tick % ticks_per_cycle) as f32 + frac;
                        (pos_ticks * 4096.0 / ticks_per_cycle as f32) % 4096.0
                    }
                } else {
                    let step = glob_lfo_step.get() / lfo_speed_mult as f32;
                    let pos = glob_lfo_pos.get();
                    if held {
                        pos
                    } else {
                        (pos + step) % 4096.0
                    }
                };

                let mut chaos = glob_chaos.get();
                chaos.tick_walks(&die);
                let sample = morph_sample(
                    next_pos as usize,
                    morph,
                    (skew, LFO_WARP, LFO_SYMMETRY),
                    0,
                    &mut chaos,
                    &die,
                );
                glob_chaos.set(chaos);

                if !held {
                    glob_lfo_pos.set(next_pos);
                }
                sample
            } else {
                prev_in_trim = clickless(prev_in_trim, glob_in_trim.get());
                let trimmed = attenuverter(raw_input, Curve::Deadzone.at(prev_in_trim));

                prev_in_offset = clickless(prev_in_offset, glob_in_offset.get());
                let in_offset = Curve::Deadzone.at(prev_in_offset) as i32 - 2047;

                let conditioned_raw = (trimmed as i32 + in_offset).clamp(0, 4095) as u16;

                let in_slew_rate = glob_in_slew.get();
                in_slew_state = slew_lin(in_slew_state, conditioned_raw, in_slew_rate, in_slew_rate);
                in_slew_state.value()
            };

            // Freeze snapshots the value so stepped morph nodes hold too, not
            // just the phase.
            if held != frozen {
                frozen = held;
                if frozen {
                    frozen_value = source_val;
                }
            }
            let input_val = if frozen { frozen_value } else { source_val };

            let midi_due = if midi_out.is_some() {
                midi_pace = midi_pace.wrapping_add(1);
                if midi_pace >= 8 {
                    midi_pace = 0;
                    midi_slot = (midi_slot + 1) % 4;
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if midi_due && midi_slot == 0 {
                let gate_val = midi_gate(input_val, nrpn);
                if gate_val != last_midi[0] {
                    midi[0].send_cc(midi_ccs[0], input_val).await;
                    last_midi[0] = gate_val;
                }
            }

            let ch0_led_color = ch0_color(lfo_active);

            // Channel 0: active source amplitude.
            paint_bipolar_level(&leds, 0, ch0_led_color, input_val);

            let muted = glob_muted.get();
            for i in 0..3 {
                let out_color = glob_modes.get()[i].color();
                let out_level = match modes[i] {
                    Mode::Cv => {
                        let att_arr = glob_att.get();
                        let offset_arr = glob_offset.get();
                        let slew_arr = glob_slew.get();

                        prev_att[i] = clickless(prev_att[i], att_arr[i]);
                        prev_offset[i] = clickless(prev_offset[i], offset_arr[i]);

                        let out_target = if muted[i] {
                            Curve::Deadzone.at(prev_offset[i])
                        } else {
                            let offset_signed = Curve::Deadzone.at(prev_offset[i]) as i32 - 2047;
                            (attenuverter(input_val, Curve::Deadzone.at(prev_att[i])) as i32
                                + offset_signed)
                                .clamp(0, 4095) as u16
                        };

                        let slew_rate = slew_arr[i];
                        out_slew_states[i] =
                            slew_lin(out_slew_states[i], out_target, slew_rate, slew_rate);
                        let out_level = out_slew_states[i].value();
                        if let OutJackType::Cv(j) = &out_jacks[i] {
                            j.set_value(out_level);
                        }
                        out_level
                    }
                    Mode::Gate => {
                        let threshold_arr = glob_threshold.get();
                        let hysteresis_arr = glob_hysteresis.get();

                        prev_threshold[i] = clickless(prev_threshold[i], threshold_arr[i]);
                        prev_hysteresis[i] = clickless(prev_hysteresis[i], hysteresis_arr[i]);

                        let threshold = prev_threshold[i];
                        let hysteresis = prev_hysteresis[i];
                        let min_gate_ms = pulse_fader_to_ms(glob_pulse.get()[i]);

                        if muted[i] {
                            if gate_high[i] {
                                gate_high[i] = false;
                                pulse_left[i] = 0;
                                if let OutJackType::Gate(g) = &out_jacks[i] {
                                    g.set_low().await;
                                }
                            }
                        } else if !gate_high[i] && input_val > threshold {
                            gate_high[i] = true;
                            pulse_left[i] = min_gate_ms;
                            if let OutJackType::Gate(g) = &out_jacks[i] {
                                g.set_high().await;
                            }
                        } else if gate_high[i]
                            && pulse_left[i] == 0
                            && input_val <= threshold.saturating_sub(hysteresis)
                        {
                            gate_high[i] = false;
                            if let OutJackType::Gate(g) = &out_jacks[i] {
                                g.set_low().await;
                            }
                        }

                        // Holds the gate up for the minimum length even if the input
                        // dips straight back below the threshold, so a fast crossing
                        // still produces a gate the receiving module can see.
                        pulse_left[i] = pulse_left[i].saturating_sub(1);

                        if gate_high[i] {
                            4095
                        } else {
                            0
                        }
                    }
                    Mode::Trigger => {
                        let threshold_arr = glob_threshold.get();
                        let hysteresis_arr = glob_hysteresis.get();
                        let pulse_arr = glob_pulse.get();

                        prev_threshold[i] = clickless(prev_threshold[i], threshold_arr[i]);
                        prev_hysteresis[i] = clickless(prev_hysteresis[i], hysteresis_arr[i]);

                        let threshold = prev_threshold[i];
                        let hysteresis = prev_hysteresis[i];
                        let pulse_ms = pulse_fader_to_ms(pulse_arr[i]);

                        if muted[i] {
                            if pulse_left[i] > 0 {
                                pulse_left[i] = 0;
                                if let OutJackType::Gate(g) = &out_jacks[i] {
                                    g.set_low().await;
                                }
                            }
                            gate_high[i] = false;
                        } else {
                            // Schmitt trigger: a slow ramp crossing the threshold
                            // must not emit a burst of triggers on input noise.
                            let high = if gate_high[i] {
                                input_val > threshold.saturating_sub(hysteresis)
                            } else {
                                input_val > threshold
                            };
                            if !gate_high[i] && high {
                                pulse_left[i] = pulse_ms;
                                if let OutJackType::Gate(g) = &out_jacks[i] {
                                    g.set_high().await;
                                }
                            }
                            gate_high[i] = high;
                        }

                        if pulse_left[i] > 0 {
                            pulse_left[i] -= 1;
                            if pulse_left[i] == 0 {
                                if let OutJackType::Gate(g) = &out_jacks[i] {
                                    g.set_low().await;
                                }
                            }
                        }

                        if pulse_left[i] > 0 {
                            4095
                        } else {
                            0
                        }
                    }
                };
                out_levels[i] = out_level;

                // `out_level` is what the jack actually carries: the shaped CV
                // in Mode::Cv, and the digital state (4095 / 0) in Gate and
                // Trigger — the same value the meters use.
                // Note edges must run every sample: short Trigger pulses can be
                // shorter than the CC slot rotation (~32 ms per lane).
                if midi_out.is_some() && midi_is_note[i + 1] {
                    let high = out_level > 0;
                    if high && !note_sounding[i + 1] {
                        midi[i + 1]
                            .send_note_on(note_pitches[i + 1], 4095)
                            .await;
                        note_sounding[i + 1] = true;
                    } else if !high && note_sounding[i + 1] {
                        midi[i + 1]
                            .send_note_off(note_pitches[i + 1])
                            .await;
                        note_sounding[i + 1] = false;
                    }
                } else if midi_due && midi_slot == i + 1 {
                    let gate_val = midi_gate(out_level, nrpn);
                    if gate_val != last_midi[i + 1] {
                        midi[i + 1].send_cc(midi_ccs[i + 1], out_level).await;
                        last_midi[i + 1] = gate_val;
                    }
                }

                // Meter colour = out mode. Dim = amplitude of that
                // channel's signal: CV uses the shaped out; Gate/Trigger flash
                // full when high and otherwise meter the conditioned input so
                // amplitude still reads while waiting for a threshold crossing.
                match modes[i] {
                    Mode::Cv => {
                        paint_bipolar_level(&leds, i + 1, out_color, out_level);
                    }
                    Mode::Gate | Mode::Trigger => {
                        if muted[i] {
                            leds.unset(i + 1, Led::Top);
                            leds.unset(i + 1, Led::Bottom);
                        } else if out_level > 0 {
                            leds.set(i + 1, Led::Top, out_color, Brightness::High);
                            leds.set(i + 1, Led::Bottom, out_color, Brightness::High);
                        } else {
                            paint_bipolar_level(&leds, i + 1, out_color, input_val);
                        }
                    }
                }
            }

            if flash[0] == 0 {
                leds.set(
                    0,
                    Led::Button,
                    ch0_led_color,
                    if held {
                        BUTTON_BRIGHTNESS
                    } else {
                        // The LFO always swings around mid-scale, regardless of
                        // the configured input range.
                        signal_brightness(input_val, lfo_active || in_range.is_bipolar())
                    },
                );
            }
            for i in 0..3 {
                if muted[i] {
                    leds.unset(i + 1, Led::Button);
                } else if flash[i + 1] == 0 {
                    if buttons.is_shift_pressed() && glob_shift_focus.get() == i + 1 {
                        leds.set(
                            i + 1,
                            Led::Button,
                            glob_modes.get()[i].color(),
                            Brightness::High,
                        );
                    } else {
                        leds.set(
                            i + 1,
                            Led::Button,
                            glob_modes.get()[i].color(),
                            signal_brightness(
                                out_levels[i],
                                modes[i] == Mode::Cv && ranges[i].is_bipolar(),
                            ),
                        );
                    }
                }
            }
        }
    };

    let fut2 = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
            app.make_latch(faders.get_value_at(2)),
            app.make_latch(faders.get_value_at(3)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let fader_val = faders.get_value_at(chan);
            glob_shift_focus.set(chan);

            if buttons.is_button_pressed(chan) && !buttons.is_shift_pressed() {
                let at_down = glob_fader_at_down.get()[chan];
                let delta = fader_val.abs_diff(at_down);
                if delta > FADER_MOVE_THRESH {
                    glob_fader_moved.modify(|m| {
                        let mut arr = *m;
                        arr[chan] = true;
                        arr
                    });
                }
            }

            let latch_active_layer =
                if buttons.is_shift_pressed() && !buttons.is_button_pressed(chan) {
                    LatchLayer::Alt
                } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(chan) {
                    LatchLayer::Third
                } else {
                    LatchLayer::Main
                };

            // Ch0 layers follow the active source: with no cable the input
            // trim / offset / slew are meaningless, so the LFO takes them over.
            let lfo_active = glob_lfo_active.get();

            let target_value = if chan == 0 {
                match (lfo_active, latch_active_layer) {
                    (true, LatchLayer::Main) => storage.query(|s| s.lfo_speed),
                    (true, LatchLayer::Alt) => storage.query(|s| s.morph),
                    (true, LatchLayer::Third) => storage.query(|s| s.skew),
                    (false, LatchLayer::Main) => storage.query(|s| s.in_trim),
                    (false, LatchLayer::Alt) => storage.query(|s| s.in_offset),
                    (false, LatchLayer::Third) => storage.query(|s| s.in_slew),
                }
            } else {
                let i = chan - 1;
                match modes[i] {
                    Mode::Cv => match latch_active_layer {
                        LatchLayer::Main => storage.query(|s| s.att[i]),
                        LatchLayer::Alt => storage.query(|s| s.offset[i]),
                        LatchLayer::Third => storage.query(|s| s.slew[i]),
                    },
                    Mode::Gate | Mode::Trigger => match latch_active_layer {
                        LatchLayer::Main => storage.query(|s| s.threshold[i]),
                        LatchLayer::Alt => storage.query(|s| s.hysteresis[i]),
                        LatchLayer::Third => storage.query(|s| s.pulse[i]),
                    },
                }
            };

            if let Some(new_value) = latch[chan].update(fader_val, latch_active_layer, target_value)
            {
                if chan == 0 {
                    match (lfo_active, latch_active_layer) {
                        (true, LatchLayer::Main) => {
                            storage.modify_and_save(|s| s.lfo_speed = new_value);
                            time_calc();
                        }
                        (true, LatchLayer::Alt) => {
                            storage.modify_and_save(|s| s.morph = new_value);
                        }
                        (true, LatchLayer::Third) => {
                            storage.modify_and_save(|s| s.skew = new_value);
                        }
                        (false, LatchLayer::Main) => {
                            storage.modify_and_save(|s| s.in_trim = new_value);
                            glob_in_trim.set(new_value);
                        }
                        (false, LatchLayer::Alt) => {
                            storage.modify_and_save(|s| s.in_offset = new_value);
                            glob_in_offset.set(new_value);
                        }
                        (false, LatchLayer::Third) => {
                            storage.modify_and_save(|s| s.in_slew = new_value);
                            glob_in_slew.set(new_value);
                        }
                    }
                } else {
                    let i = chan - 1;
                    match modes[i] {
                        Mode::Cv => match latch_active_layer {
                            LatchLayer::Main => {
                                storage.modify_and_save(|s| s.att[i] = new_value);
                                glob_att.modify(|a| {
                                    let mut arr = *a;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                            LatchLayer::Alt => {
                                storage.modify_and_save(|s| s.offset[i] = new_value);
                                glob_offset.modify(|o| {
                                    let mut arr = *o;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                            LatchLayer::Third => {
                                storage.modify_and_save(|s| s.slew[i] = new_value);
                                glob_slew.modify(|s| {
                                    let mut arr = *s;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                        },
                        Mode::Gate | Mode::Trigger => match latch_active_layer {
                            LatchLayer::Main => {
                                storage.modify_and_save(|s| s.threshold[i] = new_value);
                                glob_threshold.modify(|t| {
                                    let mut arr = *t;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                            LatchLayer::Alt => {
                                storage.modify_and_save(|s| s.hysteresis[i] = new_value);
                                glob_hysteresis.modify(|h| {
                                    let mut arr = *h;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                            LatchLayer::Third => {
                                storage.modify_and_save(|s| s.pulse[i] = new_value);
                                glob_pulse.modify(|p| {
                                    let mut arr = *p;
                                    arr[i] = new_value;
                                    arr
                                });
                            }
                        },
                    }
                }
            }
        }
    };

    // Down and up run as separate loops: every `wait_for_*` call opens its own
    // subscriber, so waiting for one channel's release inside the down handler
    // would swallow every other channel's events while a button is held.
    let button_down = async {
        loop {
            let (chan, _) = buttons.wait_for_any_down().await;
            glob_shift_focus.set(chan);

            glob_fader_at_down.modify(|a| {
                let mut arr = *a;
                arr[chan] = faders.get_value_at(chan);
                arr
            });
            glob_fader_moved.modify(|m| {
                let mut arr = *m;
                arr[chan] = false;
                arr
            });
            glob_long_press.modify(|l| {
                let mut arr = *l;
                arr[chan] = false;
                arr
            });
        }
    };

    // Long press on B / C / D: without shift cycles Mode (CV / Gate / Trigger);
    // shift+long cycles that out's jack range; run() restarts to reconfigure.
    let button_long = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_long_press().await;

            glob_long_press.modify(|l| {
                let mut arr = *l;
                arr[chan] = true;
                arr
            });

            if !(1..=3).contains(&chan) {
                continue;
            }

            let i = chan - 1;

            if shift {
                // 1 blink = ±5V, 2 blinks = 0–10V. Hold off paint + wait for
                // release and flash duration before jack restart.
                let next = match i {
                    0 => next_range(params.range_b),
                    1 => next_range(params.range_c),
                    _ => next_range(params.range_d),
                };
                let times = range_flash_times(next);
                let hold_ms = range_flash_hold_ms(times);
                leds.set_mode(
                    chan,
                    Led::Button,
                    LedMode::Flash(glob_modes.get()[i].color(), Some(times)),
                );
                glob_btn_flash.modify(|f| {
                    let mut arr = *f;
                    arr[chan] = hold_ms;
                    arr
                });
                join(
                    buttons.wait_for_up(chan),
                    app.delay_millis(hold_ms as u64),
                )
                .await;
                match i {
                    0 => params.range_b = next,
                    1 => params.range_c = next,
                    _ => params.range_d = next,
                }
                restart.signal(());
            }
            // Mode cycle is deferred to button_up so a Third-layer fader scrub
            // (btn hold + move) can cancel it — same cancel as mute.
        }
    };

    let button_up = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_up().await;

            let moved = glob_fader_moved.get()[chan];
            let long = glob_long_press.get()[chan];

            if moved {
                continue;
            }

            match chan {
                0 if shift && glob_lfo_active.get() => {
                    let clocked = storage.modify_and_save(|s| {
                        s.lfo_clocked = !s.lfo_clocked;
                        s.lfo_clocked
                    });
                    if clocked {
                        leds.set_mode(
                            0,
                            Led::Button,
                            LedMode::Flash(ch0_color(glob_lfo_active.get()), Some(4)),
                        );
                        glob_btn_flash.modify(|f| {
                            let mut arr = *f;
                            arr[0] = BUTTON_FLASH_MS;
                            arr
                        });
                    }
                }
                0 if !shift => {
                    let frozen = glob_frozen.toggle();
                    paint_buttons(
                        &leds,
                        ch0_color(glob_lfo_active.get()),
                        glob_modes.get(),
                        frozen,
                        glob_muted.get(),
                    );
                }
                // Long without Shift → Mode cycle (cancelled if fader moved).
                1..=3 if !shift && long => {
                    let i = chan - 1;
                    let next_mode = match i {
                        0 => {
                            params.mode_b = params.mode_b.next();
                            params.mode_b
                        }
                        1 => {
                            params.mode_c = params.mode_c.next();
                            params.mode_c
                        }
                        _ => {
                            params.mode_d = params.mode_d.next();
                            params.mode_d
                        }
                    };
                    glob_modes.modify(|m| {
                        let mut arr = *m;
                        arr[i] = next_mode;
                        arr
                    });
                    leds.set_mode(
                        chan,
                        Led::Button,
                        LedMode::Flash(next_mode.color(), Some(4)),
                    );
                    glob_btn_flash.modify(|f| {
                        let mut arr = *f;
                        arr[chan] = BUTTON_FLASH_MS;
                        arr
                    });
                    restart.signal(());
                }
                // Short tap → mute. Shift reserved for range swap.
                1..=3 if !shift => {
                    let i = chan - 1;
                    let muted = storage.modify_and_save(|s| {
                        s.muted[i] = !s.muted[i];
                        s.muted[i]
                    });
                    let muted_all = glob_muted.modify(|m| {
                        let mut arr = *m;
                        arr[i] = muted;
                        arr
                    });
                    paint_buttons(
                        &leds,
                        ch0_color(glob_lfo_active.get()),
                        glob_modes.get(),
                        glob_frozen.get(),
                        muted_all,
                    );
                }
                _ => {}
            }
        }
    };

    let fut3 = join3(button_down, button_up, button_long);

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;

                    let (
                        in_trim,
                        in_offset,
                        in_slew,
                        att,
                        offset,
                        slew,
                        threshold,
                        hysteresis,
                        pulse,
                        muted,
                    ) = storage.query(|s| {
                        (
                            s.in_trim,
                            s.in_offset,
                            s.in_slew,
                            s.att,
                            s.offset,
                            s.slew,
                            s.threshold,
                            s.hysteresis,
                            s.pulse,
                            s.muted,
                        )
                    });

                    glob_in_trim.set(in_trim);
                    glob_in_offset.set(in_offset);
                    glob_in_slew.set(in_slew);
                    glob_att.set(att);
                    glob_offset.set(offset);
                    glob_slew.set(slew);
                    glob_threshold.set(threshold);
                    glob_hysteresis.set(hysteresis);
                    glob_pulse.set(pulse);
                    glob_muted.set(muted);

                    time_calc();

                    paint_buttons(
                        &leds,
                        ch0_color(glob_lfo_active.get()),
                        glob_modes.get(),
                        glob_frozen.get(),
                        muted,
                    );
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    let clock_drain = async {
        loop {
            if let ClockEvent::Tick(tick) = clock.wait_for_event(ClockDivision::_1).await {
                glob_ticks.set(tick);
            }
        }
    };

    select(join4(fut1, fut2, fut3, scene_handler), join(clock_drain, restart.wait())).await;
}

mod morph {
    #![allow(dead_code)]

    use libfp::{Color, Waveform};

    use crate::app::Die;

    pub const MORPH_NODES: usize = 7;
    const NODE_HUES: [u16; MORPH_NODES] = [0, 45, 90, 135, 180, 225, 270];
    const SYMMETRY_LEAN_CURVE: f32 = 0.45;

    pub fn symmetry_phase(phase: usize, symmetry: u16) -> usize {
        let t = (phase % 4096) as f32 / 4096.0;
        let centered = (symmetry as f32 / 4095.0 - 0.5) * 2.0;
        let lean = libm::copysignf(libm::powf(centered.abs(), SYMMETRY_LEAN_CURVE), centered);
        let pw = (0.5 + lean * 0.49).clamp(0.01, 0.99);
        let out = if t < pw {
            t / pw * 0.5
        } else {
            0.5 + (t - pw) / (1.0 - pw) * 0.5
        };
        (out.clamp(0.0, 1.0) * 4095.0) as usize
    }

    pub fn warp_phase(phase: usize, warp: u16) -> usize {
        if warp == 0 {
            return phase % 4096;
        }
        let t = (phase % 4096) as f32 / 4096.0;
        let amount = warp as f32 / 4095.0;
        let eased = t * t * (3.0 - 2.0 * t);
        let out = t * (1.0 - amount) + eased * amount;
        (out * 4095.0) as usize
    }

    pub fn skew_phase(phase: usize, skew: u16) -> usize {
        let t = (phase % 4096) as f32 / 4096.0;
        let s = (skew as f32 / 4095.0 - 0.5) * 2.0;
        let warped = if s >= 0.0 {
            libm::powf(t, 1.0 + s)
        } else {
            1.0 - libm::powf(1.0 - t, 1.0 - s)
        };
        (warped.clamp(0.0, 1.0) * 4095.0) as usize
    }

    #[derive(Clone, Copy)]
    pub struct MorphChaos {
        walk_a: i32,
        walk_b: i32,
        sh_a: u16,
        sh_b: u16,
        sh_bucket_a: u16,
        sh_bucket_b: u16,
    }

    impl MorphChaos {
        pub fn new() -> Self {
            Self {
                walk_a: 2048,
                walk_b: 2048,
                sh_a: 2048,
                sh_b: 2048,
                sh_bucket_a: 0xffff,
                sh_bucket_b: 0xffff,
            }
        }

        pub fn tick_walks(&mut self, die: &Die) {
            let step_a = (die.roll() as i32 % 7) - 3;
            let step_b = (die.roll() as i32 % 7) - 3;
            self.walk_a = (self.walk_a + step_a).clamp(0, 4095);
            self.walk_b = (self.walk_b + step_b).clamp(0, 4095);
        }
    }

    fn classic_wave(node: usize, phase: usize) -> Option<u16> {
        let w = match node {
            0 => Waveform::Sine,
            1 => Waveform::Triangle,
            2 => Waveform::Saw,
            3 => Waveform::Square,
            _ => return None,
        };
        Some(w.at(phase))
    }

    fn chaos_sample(node: usize, phase: usize, osc: usize, chaos: &mut MorphChaos, die: &Die) -> u16 {
        match node {
            4 => {
                if osc == 0 {
                    chaos.walk_a as u16
                } else {
                    chaos.walk_b as u16
                }
            }
            5 => {
                let bucket = (phase / 256) as u16;
                let (sh, last) = if osc == 0 {
                    (&mut chaos.sh_a, &mut chaos.sh_bucket_a)
                } else {
                    (&mut chaos.sh_b, &mut chaos.sh_bucket_b)
                };
                if bucket != *last {
                    *last = bucket;
                    *sh = die.roll();
                }
                *sh
            }
            _ => die.roll(),
        }
    }

    fn node_sample(node: usize, phase: usize, osc: usize, chaos: &mut MorphChaos, die: &Die) -> u16 {
        classic_wave(node, phase).unwrap_or_else(|| chaos_sample(node, phase, osc, chaos, die))
    }

    pub fn morph_sample(
        phase: usize,
        morph: u16,
        form: (u16, u16, u16),
        osc: usize,
        chaos: &mut MorphChaos,
        die: &Die,
    ) -> u16 {
        let (skew, warp, symmetry) = form;
        let p = skew_phase(symmetry_phase(warp_phase(phase, warp), symmetry), skew);
        let segments = MORPH_NODES - 1;
        let seg_size = 4096 / segments;
        let raw_seg = (morph as usize) / seg_size;
        if raw_seg >= segments {
            return node_sample(MORPH_NODES - 1, p, osc, chaos, die);
        }
        let frac = (morph as usize) % seg_size;
        let a = node_sample(raw_seg, p, osc, chaos, die) as i32;
        let b = node_sample(raw_seg + 1, p, osc, chaos, die) as i32;
        (a + (b - a) * frac as i32 / seg_size as i32).clamp(0, 4095) as u16
    }

    fn hsv_to_rgb(hue: u16) -> (u8, u8, u8) {
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

    fn hsv_to_rgb_sat(hue: u16, sat: u8) -> (u8, u8, u8) {
        let (fr, fg, fb) = hsv_to_rgb(hue);
        let s = sat as u32;
        let w = 255 - s;
        (
            ((fr as u32 * s + 255 * w) / 255) as u8,
            ((fg as u32 * s + 255 * w) / 255) as u8,
            ((fb as u32 * s + 255 * w) / 255) as u8,
        )
    }

    pub fn morph_color(morph: u16) -> Color {
        let segments = MORPH_NODES - 1;
        let seg_size = 4096 / segments;
        let m = morph.min(4095) as usize;
        let seg = (m / seg_size).min(segments - 1);
        let frac = m % seg_size;
        if seg == segments - 1 {
            let (r, g, b) = hsv_to_rgb(NODE_HUES[MORPH_NODES - 1]);
            return Color::Custom(r, g, b);
        }
        let sat = 255u8.saturating_sub((frac * 255 / seg_size) as u8);
        let (r, g, b) = hsv_to_rgb_sat(NODE_HUES[seg], sat);
        Color::Custom(r, g, b)
    }
}
