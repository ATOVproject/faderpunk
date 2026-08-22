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
        attenuate_bipolar, attenuverter, clickless, midi_gate, slew_lin, split_unsigned_value, SlewState,
    },
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiCc, MidiChannel, MidiOut, Param, Range, Value,
    APP_MAX_PARAMS,
};

use crate::{
    app::{
        App, AppParams, AppStorage, ClockEvent, Led, Leds, ManagedStorage, MidiOutput, SceneEvent,
    },
    tasks::leds::LedMode,
};

use self::morph::{morph_sample, MorphChaos};


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


const CLOCK_DIVISIONS: [u32; 13] = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2, 1];

fn division_at(fader: u16, count: usize) -> u32 {
    let count = count.clamp(1, CLOCK_DIVISIONS.len());
    let idx = (fader.min(4095) as usize * count / 4096).min(count - 1);
    CLOCK_DIVISIONS[idx]
}

fn lfo_step(speed: u16) -> f32 {
    (Curve::Exponential.at(speed) as f32 + 2047.0 - 2047.0) * 0.015 + 0.0682
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

pub const CHANNELS: usize = 4;
pub const PARAMS: usize = 15;

/// `Ch Map` packs one MIDI channel per wave into a nibble (wave 0 = bits 0..3,
/// nibble + 1 = channel 1..16). A whole map of 0 means "every wave follows the
/// base MIDI Channel", which is the shipped default.
const CH_MAP_FOLLOW: i32 = 0;
const CH_MAP_MAX: i32 = (1 << (4 * CHANNELS)) - 1;
/// `CC Map` packs one CC number per wave into 7 bits (wave 0 = bits 0..6). A
/// whole map of 0 means "every wave follows the base MIDI CC" via the
/// base + offset derivation, which is the shipped default.
const CC_MAP_FOLLOW: i32 = 0;
const CC_MAP_MAX: i32 = (1 << (7 * CHANNELS)) - 1;
/// Keeps the literal bounds in `CONFIG` honest against the packing above.
const _: () = assert!(CH_MAP_FOLLOW == 0 && CH_MAP_MAX == 65_535);
const _: () = assert!(CC_MAP_FOLLOW == 0 && CC_MAP_MAX == 268_435_455);

/// DSP loop period. 8 ms rather than 1 ms: a 1 kHz loop that also mirrors MIDI
/// starves the config SysEx path in dense layouts.
const AUDIO_MS: u16 = 8;
const FADER_MOVE_THRESH: u16 = 64;
const BUTTON_BRIGHTNESS: Brightness = Brightness::Mid;
const BUTTON_IDLE_BRIGHTNESS: Brightness = Brightness::Low;
/// Input samples within this 12-bit distance count as unchanged (ADC noise floor).
const IN_DEADBAND: u16 = 24;
/// Milliseconds of unchanged input before the root falls back to the internal
/// LFO.
const IN_IDLE_MS: u16 = 1200;
/// Hold off periodic button LED writes so LedMode::Flash can finish.
const BUTTON_FLASH_MS: u16 = 848;
/// One `LedMode::Flash` cycle ≈ 16 frames at 60 Hz.
const RANGE_FLASH_CYCLE_MS: u16 = 270;
/// Warp default for the root morph axis (Third on Ch0 is skew).
const LFO_WARP: u16 = 0;
/// Fixed symmetry for the idle root LFO.
const LFO_SYMMETRY: u16 = 2048;
/// Clock divisions the root speed fader spans.
const LFO_DIVISIONS: usize = 9;
/// Five process hues. Ch0 CV (200) and Ch0 LFO (238) match Manifold; Fold /
/// Soft / Rect fan -38 deg: 162 / 124 / 86. Steady LEDs follow glob_process.
const RIPPPPLE_HUES: [u16; 5] = [200, 238, 162, 124, 86];

fn ripppple_color(step: usize) -> Color {
    let (r, g, b) = hsv_to_rgb(RIPPPPLE_HUES[step.min(4)] % 360);
    Color::Custom(r, g, b)
}

fn ch0_color(lfo_active: bool) -> Color {
    if lfo_active {
        ripppple_color(1)
    } else {
        ripppple_color(0)
    }
}

/// Waveshaper applied at each stage of the signal chain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Process {
    Fold,
    Soft,
    Rect,
}

impl Process {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Process::Soft,
            2 => Process::Rect,
            _ => Process::Fold,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Process::Fold => 0,
            Process::Soft => 1,
            Process::Rect => 2,
        }
    }

    fn next(self) -> Self {
        match self {
            Process::Fold => Process::Soft,
            Process::Soft => Process::Rect,
            Process::Rect => Process::Fold,
        }
    }

    /// Button / meter hue for this process — persistent, not channel-based.
    fn color(self) -> Color {
        match self {
            Process::Fold => ripppple_color(2),
            Process::Soft => ripppple_color(3),
            Process::Rect => ripppple_color(4),
        }
    }
}


fn s_curve_12bit(value: u16) -> u16 {
    let x = value.min(4095) as u64;
    let x2 = x * x;
    let num = 3 * x2 * 4095 - 2 * x2 * x;
    let den = 4095u64 * 4095;
    ((num + den / 2) / den).min(4095) as u16
}

fn rectify_12bit(value: u16) -> u16 {
    value.abs_diff(2047).saturating_add(2047).min(4095)
}

fn fold_12bit(mut value: i32) -> u16 {
    for _ in 0..64 {
        if (0..=4095).contains(&value) {
            return value as u16;
        }
        if value < 0 {
            value = -value;
        } else {
            value = 8190 - value;
        }
    }
    value.clamp(0, 4095) as u16
}

fn lerp_12bit(a: u16, b: u16, amount: u16) -> u16 {
    let t = amount as i32;
    ((a as i32 * (4095 - t) + b as i32 * t) / 4095) as u16
}

fn stage_shaped(input: u16, process: Process, amount: u16) -> u16 {
    match process {
        Process::Fold => {
            let mid = 2047;
            let x = input as i32 - mid;
            let gain = 1.0 + (amount as f32 / 4095.0) * 7.0;
            fold_12bit(mid + (x as f32 * gain) as i32)
        }
        Process::Soft => lerp_12bit(input, s_curve_12bit(input), amount),
        Process::Rect => lerp_12bit(input, rectify_12bit(input), amount),
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

fn paint_bipolar_level(leds: &Leds<CHANNELS>, chan: usize, color: Color, level: u16) {
    let parts = split_unsigned_value(level);
    leds.set(chan, Led::Top, color, Brightness::Custom(parts[0]));
    leds.set(chan, Led::Bottom, color, Brightness::Custom(parts[1]));
}

fn paint_buttons(
    leds: &Leds<CHANNELS>,
    in_color: Color,
    process: [u8; 3],
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
            leds.set(
                i + 1,
                Led::Button,
                Process::from_u8(process[i]).color(),
                BUTTON_BRIGHTNESS,
            );
        }
    }
}

pub static CONFIG: Config<PARAMS> = Config::new(
    "Ripppple",
    "Root CV or LFO through three cumulative waveshapers",
    Color::Cyan,
    AppIcon::Sine,
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
.add_param(Param::Range {
    name: "Range B",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Range {
    name: "Range C",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Range {
    name: "Range D",
    variants: &[Range::_0_10V, Range::_Neg5_5V],
})
.add_param(Param::Enum {
    name: "Process B",
    variants: &["Fold", "Soft", "Rect"],
})
.add_param(Param::Enum {
    name: "Process C",
    variants: &["Fold", "Soft", "Rect"],
})
.add_param(Param::Enum {
    name: "Process D",
    variants: &["Fold", "Soft", "Rect"],
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
});

pub struct Params {
    color: Color,
    in_range: Range,
    range_b: Range,
    range_c: Range,
    range_d: Range,
    /// Start value of the per-stage waveshaper (Ch1..Ch3); runtime state
    /// lives in `Storage::process`.
    process: [usize; 3],
    lfo_speed_mult: usize,
    midi_out: MidiOut,
    midi_channel: MidiChannel,
    /// Base CC; the four channels take base + 0..=3.
    midi_cc: MidiCc,
    nrpn: bool,
    /// Four packed nibbles, one channel per wave. See `CH_MAP_FOLLOW`.
    ch_map: i32,
    /// Four packed 7-bit fields, one CC per wave. See `CC_MAP_FOLLOW`.
    cc_map: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            color: Color::Cyan,
            in_range: Range::_Neg5_5V,
            range_b: Range::_Neg5_5V,
            range_c: Range::_Neg5_5V,
            range_d: Range::_Neg5_5V,
            process: [0; 3],
            lfo_speed_mult: 0,
            midi_out: MidiOut([false; 3]),
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32u8),
            nrpn: false,
            ch_map: CH_MAP_FOLLOW,
            cc_map: CC_MAP_FOLLOW,
        }
    }
}

impl Params {
    /// Resolve the sending MIDI channel for one wave.
    fn channel_for(&self, wave: usize) -> MidiChannel {
        if self.ch_map == CH_MAP_FOLLOW {
            return self.midi_channel;
        }
        let nibble = ((self.ch_map >> (4 * wave)) & 0xF) as u8;
        MidiChannel::from(nibble + 1)
    }

    /// Resolve the CC number for one wave.
    fn cc_for(&self, wave: usize, nrpn: bool) -> MidiCc {
        if self.cc_map == CC_MAP_FOLLOW {
            return channel_cc(self.midi_cc, wave, nrpn);
        }
        let field = ((self.cc_map >> (7 * wave)) & 0x7F) as u16;
        MidiCc::from(field.min(midi_cc_limit(nrpn)))
    }
}

/// Highest CC number the transport can carry: NRPN uses the full 14-bit
/// parameter space, plain CC only 7 bit.
const fn midi_cc_limit(nrpn: bool) -> u16 {
    if nrpn {
        16383
    } else {
        127
    }
}

fn channel_cc(base: MidiCc, chan: usize, nrpn: bool) -> MidiCc {
    MidiCc::from(
        base.as_u16()
            .saturating_add(chan as u16)
            .min(midi_cc_limit(nrpn)),
    )
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // Tolerant of short legacy slices: missing tail params fall back to the
        // silent / neutral defaults rather than panicking.
        if values.is_empty() {
            return None;
        }
        let at = |i: usize| values.get(i).copied();
        // Legacy layouts stored Source at index 8; skip it when the slice is
        // still 16-wide or when index 9 still holds the old LFO Speed enum.
        let legacy_source = values.len() >= 16
            || matches!(values.get(9), Some(Value::Enum(_)))
                && values.get(10).is_some();
        let tail = if legacy_source { 1 } else { 0 };
        Some(Self {
            color: Color::from_value(values[0]),
            in_range: at(1).map(Range::from_value).unwrap_or(Range::_Neg5_5V),
            range_b: at(2).map(Range::from_value).unwrap_or(Range::_Neg5_5V),
            range_c: at(3).map(Range::from_value).unwrap_or(Range::_Neg5_5V),
            range_d: at(4).map(Range::from_value).unwrap_or(Range::_Neg5_5V),
            process: [
                at(5).map(usize::from_value).unwrap_or(0).min(2),
                at(6).map(usize::from_value).unwrap_or(0).min(2),
                at(7).map(usize::from_value).unwrap_or(0).min(2),
            ],
            lfo_speed_mult: at(8 + tail).map(usize::from_value).unwrap_or(0),
            midi_out: at(9 + tail)
                .map(MidiOut::from_value)
                .unwrap_or(MidiOut([false; 3])),
            midi_channel: at(10 + tail)
                .map(MidiChannel::from_value)
                .unwrap_or_default(),
            midi_cc: at(11 + tail)
                .map(MidiCc::from_value)
                .unwrap_or(MidiCc::from(32u8)),
            nrpn: at(12 + tail).map(bool::from_value).unwrap_or(false),
            ch_map: at(13 + tail)
                .map(i32::from_value)
                .unwrap_or(CH_MAP_FOLLOW)
                .clamp(CH_MAP_FOLLOW, CH_MAP_MAX),
            cc_map: at(14 + tail)
                .map(i32::from_value)
                .unwrap_or(CC_MAP_FOLLOW)
                .clamp(CC_MAP_FOLLOW, CC_MAP_MAX),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.color.into()).unwrap();
        vec.push(self.in_range.into()).unwrap();
        vec.push(self.range_b.into()).unwrap();
        vec.push(self.range_c.into()).unwrap();
        vec.push(self.range_d.into()).unwrap();
        for p in self.process {
            vec.push(Value::Enum(p)).unwrap();
        }
        vec.push(Value::Enum(self.lfo_speed_mult)).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(Value::MidiNrpn(self.nrpn)).unwrap();
        vec.push(self.ch_map.into()).unwrap();
        vec.push(self.cc_map.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    /// Per stage: waveshape amount, DC bias, output slew, and process mode.
    amount: [u16; 3],
    offset: [u16; 3],
    slew: [u16; 3],
    process: [u8; 3],
    muted: [bool; 3],
    /// Root LFO layers (Main / Alt / Third) plus its clock sync.
    lfo_speed: u16,
    morph: u16,
    skew: u16,
    lfo_clocked: bool,
    /// CV-in conditioning when the jack is live.
    in_trim: u16,
    in_offset: u16,
    in_slew: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            amount: [0; 3],
            offset: [2048; 3],
            slew: [0; 3],
            process: [Process::Fold.as_u8(); 3],
            muted: [false; 3],
            lfo_speed: 2000,
            morph: 0,
            skew: 2048,
            lfo_clocked: false,
            in_trim: 4095,
            in_offset: 2048,
            in_slew: 0,
        }
    }
}

impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let mut params = Params {
        color: Color::Cyan,
        in_range: Range::_Neg5_5V,
        range_b: Range::_Neg5_5V,
        range_c: Range::_Neg5_5V,
        range_d: Range::_Neg5_5V,
        process: [0; 3],
        lfo_speed_mult: 0,
        midi_out: MidiOut([false; 3]),
        midi_channel: MidiChannel::default(),
        midi_cc: MidiCc::from(32u8.saturating_add(app.start_channel as u8)),
        ..Default::default()
    };
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    storage.load().await;

    let app_loop = async {
        loop {
            select(run(&app, &mut params, &storage), storage.saver_task()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &mut Params,
    storage: &ManagedStorage<Storage>,
) {
    let in_range = params.in_range;
    let range_b = params.range_b;
    let range_c = params.range_c;
    let range_d = params.range_d;
    let midi_out = params.midi_out;
    let nrpn = params.nrpn;
    let midi_chans = core::array::from_fn::<_, CHANNELS, _>(|w| params.channel_for(w));
    let midi_ccs = core::array::from_fn::<_, CHANNELS, _>(|w| params.cc_for(w, params.nrpn));
    let lfo_speed_mult = 2u32.pow(params.lfo_speed_mult.min(31) as u32);

    // Configurator "Process B/C/D" are start values; applied once per run() (a
    // host param edit restarts run). A scene load overrides storage later.
    let p_process = params.process;
    storage.modify_and_save(|s| {
        for (slot, p) in s.process.iter_mut().zip(p_process.iter()) {
            *slot = (*p).min(2) as u8;
        }
    });

    let ranges = [range_b, range_c, range_d];
    let bipolar = [
        range_b.is_bipolar(),
        range_c.is_bipolar(),
        range_d.is_bipolar(),
    ];
    let initial_lfo_active = true;

    let midi: [MidiOutput; CHANNELS] =
        core::array::from_fn(|w| app.use_midi_output(midi_out, midi_chans[w], nrpn));

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let glob_amount = app.make_global([0u16; 3]);
    let glob_offset = app.make_global([2048u16; 3]);
    let glob_slew = app.make_global([0u16; 3]);
    let glob_process = app.make_global([0u8; 3]);
    let glob_muted = app.make_global([false; 3]);
    let glob_in_trim = app.make_global(4095u16);
    let glob_in_offset = app.make_global(2048u16);
    let glob_in_slew = app.make_global(0u16);
    let glob_frozen = app.make_global(false);

    let glob_fader_at_down = app.make_global([0u16; 4]);
    let glob_fader_moved = app.make_global([false; 4]);
    let glob_long_press = app.make_global([false; 4]);

    // Signalled when an out range changes: run() has to return so the jack is
    // reconfigured.
    let restart = Signal::<NoopRawMutex, ()>::new();

    let die = app.use_die();
    let mut clock = app.use_clock();
    let glob_ticks = app.make_global(0u64);
    let glob_lfo_active = app.make_global(true);
    let glob_lfo_step = app.make_global(0.0682f32);
    let glob_div = app.make_global(24u32);
    let glob_btn_flash = app.make_global([0u16; 4]);

    let time_calc = || {
        let speed = storage.query(|s| s.lfo_speed);
        glob_lfo_step.set(lfo_step(speed));
        glob_div.set(division_at(speed, LFO_DIVISIONS));
    };
    time_calc();

    let (amount, offset, slew, process, muted, in_trim, in_offset, in_slew) = storage.query(|s| {
        (
            s.amount,
            s.offset,
            s.slew,
            s.process,
            s.muted,
            s.in_trim,
            s.in_offset,
            s.in_slew,
        )
    });

    glob_amount.set(amount);
    glob_offset.set(offset);
    glob_slew.set(slew);
    glob_process.set(process);
    glob_muted.set(muted);
    glob_in_trim.set(in_trim);
    glob_in_offset.set(in_offset);
    glob_in_slew.set(in_slew);

    let in_jack = app.make_in_jack(0, in_range).await;
    let out_jacks = [
        app.make_out_jack(1, ranges[0]).await,
        app.make_out_jack(2, ranges[1]).await,
        app.make_out_jack(3, ranges[2]).await,
    ];

    paint_buttons(&leds, ch0_color(initial_lfo_active), process, false, muted);

    let fut1 = async {
        let mut root_pos = 0.0f32;
        let mut root_chaos = MorphChaos::new();
        let mut mute_gain = [4095u16; 3];
        let mut out_levels = [0u16; 3];
        let mut frozen = false;
        let mut frozen_value = 0u16;
        // u16::MAX is out of `midi_gate`'s range and forces one send per
        // channel on startup.
        let mut last_midi = [u16::MAX; 4];
        // One channel offered per iteration and rotated: at 8 ms that is at
        // most 125 messages/s in total, each channel refreshing at ~31 Hz.
        let mut midi_slot: usize = 0;

        let mut prev_raw_input = in_jack.get_value();
        // Starts idle so an unpatched Ripppple comes up on the internal LFO.
        let mut in_idle_ms = IN_IDLE_MS;
        let mut last_tick = glob_ticks.get();
        let mut ms_since_tick = 0u16;
        let mut tick_period_ms = 21u16;

        let mut in_slew_state = SlewState::new();
        let mut stage_slew_states = [SlewState::new(); 3];
        let mut prev_in_trim = in_trim;
        let mut prev_in_offset = in_offset;

        loop {
            app.delay_millis(AUDIO_MS as u64).await;

            let raw_input = in_jack.get_value();

            // Patch heuristic: the jack has no sense pin, so a live cable is
            // inferred from movement. Deadband covers the ADC noise floor.
            if raw_input.abs_diff(prev_raw_input) > IN_DEADBAND {
                in_idle_ms = 0;
            } else {
                in_idle_ms = in_idle_ms.saturating_add(AUDIO_MS).min(IN_IDLE_MS);
            }
            prev_raw_input = raw_input;

            let lfo_active = in_idle_ms >= IN_IDLE_MS;
            glob_lfo_active.set(lfo_active);

            let flash = glob_btn_flash.modify(|f| {
                let mut arr = *f;
                for ms in arr.iter_mut() {
                    *ms = ms.saturating_sub(AUDIO_MS);
                }
                arr
            });

            let (morph, skew, lfo_clocked) = storage.query(|s| (s.morph, s.skew, s.lfo_clocked));

            let tick = glob_ticks.get();
            if tick != last_tick {
                // Plausible tick gaps only: ignore the counter reset to u64::MAX
                // and anything slower than 2 s, which would be a stopped clock.
                if (AUDIO_MS..500).contains(&ms_since_tick) && tick > last_tick {
                    tick_period_ms = ms_since_tick;
                }
                last_tick = tick;
                ms_since_tick = 0;
            } else {
                ms_since_tick = ms_since_tick.saturating_add(AUDIO_MS);
            }

            // Keep the random-walk drift at its ~1 kHz rate even though the
            // loop only runs every AUDIO_MS.
            for _ in 0..AUDIO_MS {
                root_chaos.tick_walks(&die);
            }

            let held = glob_frozen.get();

            let root_sample = if lfo_active {
                let next_pos = if lfo_clocked {
                    // Phase locked to the tick counter, interpolated inside the
                    // tick. A stopped transport stops the counter, which parks
                    // the wave by itself.
                    if tick == u64::MAX {
                        0.0
                    } else {
                        let ticks_per_cycle = (glob_div.get() as u64)
                            .saturating_mul(lfo_speed_mult as u64)
                            .max(1);
                        let frac = (ms_since_tick as f32 / tick_period_ms.max(1) as f32).min(1.0);
                        let pos_ticks = (tick % ticks_per_cycle) as f32 + frac;
                        (pos_ticks * 4096.0 / ticks_per_cycle as f32) % 4096.0
                    }
                } else {
                    let step =
                        glob_lfo_step.get() * AUDIO_MS as f32 / lfo_speed_mult as f32;
                    if held {
                        root_pos
                    } else {
                        (root_pos + step) % 4096.0
                    }
                };

                let sample = morph_sample(
                    next_pos as usize,
                    morph,
                    (skew, LFO_WARP, LFO_SYMMETRY),
                    0,
                    &mut root_chaos,
                    &die,
                );

                if !held {
                    root_pos = next_pos;
                }
                sample
            } else {
                prev_in_trim = clickless(prev_in_trim, glob_in_trim.get());
                let trimmed = attenuverter(raw_input, Curve::Deadzone.at(prev_in_trim));

                prev_in_offset = clickless(prev_in_offset, glob_in_offset.get());
                let offset_signed = Curve::Deadzone.at(prev_in_offset) as i32 - 2047;

                let conditioned_raw =
                    (trimmed as i32 + offset_signed).clamp(0, 4095) as u16;

                let in_slew_rate = glob_in_slew.get();
                in_slew_state = slew_lin(
                    in_slew_state,
                    conditioned_raw,
                    in_slew_rate,
                    in_slew_rate,
                );
                in_slew_state.value()
            };

            // Freeze snapshots the value so stepped morph nodes hold too, not
            // just the phase.
            if held != frozen {
                frozen = held;
                if frozen {
                    frozen_value = root_sample;
                }
            }
            let root_val = if frozen { frozen_value } else { root_sample };

            let midi_due = midi_out.is_some();
            if midi_due {
                midi_slot = (midi_slot + 1) % 4;
            }

            if midi_due && midi_slot == 0 {
                let gate_val = midi_gate(root_val, nrpn);
                if gate_val != last_midi[0] {
                    midi[0].send_cc(midi_ccs[0], root_val).await;
                    last_midi[0] = gate_val;
                }
            }

            let ch0_led_color = ch0_color(lfo_active);
            paint_bipolar_level(&leds, 0, ch0_led_color, root_val);

            let amount = glob_amount.get();
            let offset = glob_offset.get();
            let slew = glob_slew.get();
            let process = glob_process.get();
            let muted = glob_muted.get();

            // Signal chain: each stage shapes its direct predecessor.
            let mut modulator = root_val;
            for i in 0..3 {
                let proc = Process::from_u8(process[i]);
                let shaped = stage_shaped(modulator, proc, amount[i]);
                let biased =
                    (shaped as i32 + (offset[i] as i32 - 2047)).clamp(0, 4095) as u16;
                stage_slew_states[i] =
                    slew_lin(stage_slew_states[i], biased, slew[i], slew[i]);
                let slewed = stage_slew_states[i].value();

                // Ramped rather than switched, so a mute does not click.
                // Mute only this jack — the chain still feeds the next stage.
                mute_gain[i] = clickless(mute_gain[i], if muted[i] { 0 } else { 4095 });
                let level = if bipolar[i] {
                    attenuate_bipolar(slewed, mute_gain[i])
                } else {
                    ((slewed as u32 * mute_gain[i] as u32) / 4095) as u16
                };

                out_jacks[i].set_value(level);
                out_levels[i] = level;
                modulator = slewed;

                if midi_due && midi_slot == i + 1 {
                    let gate_val = midi_gate(level, nrpn);
                    if gate_val != last_midi[i + 1] {
                        midi[i + 1].send_cc(midi_ccs[i + 1], level).await;
                        last_midi[i + 1] = gate_val;
                    }
                }

                let color = proc.color();
                if muted[i] {
                    leds.set(i + 1, Led::Top, color, Brightness::Low);
                    leds.set(i + 1, Led::Bottom, color, Brightness::Low);
                } else {
                    paint_bipolar_level(&leds, i + 1, color, level);
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
                        signal_brightness(root_val, lfo_active || in_range.is_bipolar())
                    },
                );
            }

            // Keep metering while Shift holds Alt so the stage output (and
            // Offset edits) stay visible under the Alt latch.
            for i in 0..3 {
                if muted[i] {
                    leds.unset(i + 1, Led::Button);
                } else if flash[i + 1] == 0 {
                    leds.set(
                        i + 1,
                        Led::Button,
                        Process::from_u8(process[i]).color(),
                        signal_brightness(out_levels[i], bipolar[i]),
                    );
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

            if buttons.is_button_pressed(chan) && !buttons.is_shift_pressed() {
                let at_down = glob_fader_at_down.get()[chan];
                if fader_val.abs_diff(at_down) > FADER_MOVE_THRESH {
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
                match latch_active_layer {
                    LatchLayer::Main => storage.query(|s| s.amount[i]),
                    LatchLayer::Alt => storage.query(|s| s.offset[i]),
                    LatchLayer::Third => storage.query(|s| s.slew[i]),
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
                    match latch_active_layer {
                        LatchLayer::Main => {
                            storage.modify_and_save(|s| s.amount[i] = new_value);
                            glob_amount.modify(|a| {
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
                            glob_slew.modify(|sl| {
                                let mut arr = *sl;
                                arr[i] = new_value;
                                arr
                            });
                        }
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
                // 1 blink = ±5V, 2 blinks = 0–10V. Flash before restart so the
                // paint loop's hold-off can show it; wait for release and the
                // flash duration before reconfiguring the jack.
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
                    LedMode::Flash(Process::from_u8(glob_process.get()[i]).color(), Some(times)),
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
            // Process cycle is deferred to button_up so a Third-layer fader
            // scrub (btn hold + move) can cancel it — same cancel as mute.
        }
    };

    let button_up = async {
        loop {
            let (chan, shift) = buttons.wait_for_any_up().await;

            let moved = glob_fader_moved.get()[chan];
            let long = glob_long_press.get()[chan];

            // Third-layer scrub: ignore short mute and deferred Process cycle.
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
                        glob_process.get(),
                        frozen,
                        glob_muted.get(),
                    );
                }
                // Long without Shift → Process cycle (cancelled if fader moved).
                1..=3 if !shift && long => {
                    let i = chan - 1;
                    let next = storage.modify_and_save(|s| {
                        s.process[i] = Process::from_u8(s.process[i]).next().as_u8();
                        s.process[i]
                    });
                    glob_process.modify(|p| {
                        let mut arr = *p;
                        arr[i] = next;
                        arr
                    });
                    leds.set_mode(
                        chan,
                        Led::Button,
                        LedMode::Flash(Process::from_u8(next).color(), Some(4)),
                    );
                    glob_btn_flash.modify(|f| {
                        let mut arr = *f;
                        arr[chan] = BUTTON_FLASH_MS;
                        arr
                    });
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
                        glob_process.get(),
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

                    let (amount, offset, slew, process, muted, in_trim, in_offset, in_slew) =
                        storage.query(|s| {
                            (
                                s.amount,
                                s.offset,
                                s.slew,
                                s.process,
                                s.muted,
                                s.in_trim,
                                s.in_offset,
                                s.in_slew,
                            )
                        });

                    glob_amount.set(amount);
                    glob_offset.set(offset);
                    glob_slew.set(slew);
                    glob_process.set(process);
                    glob_muted.set(muted);
                    glob_in_trim.set(in_trim);
                    glob_in_offset.set(in_offset);
                    glob_in_slew.set(in_slew);

                    time_calc();

                    paint_buttons(
                        &leds,
                        ch0_color(glob_lfo_active.get()),
                        process,
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

    select(
        join4(fut1, fut2, fut3, scene_handler),
        join(clock_drain, restart.wait()),
    )
    .await;
}


mod morph {
    #![allow(dead_code)]

    use libfp::{Color, Waveform};

    use crate::app::Die;

/// Morph continuum: soft waves → stepped/chaos.
/// Indices: 0 Sine, 1 Tri, 2 Saw, 3 Square, 4 Walk, 5 S&H, 6 Noise
pub const MORPH_NODES: usize = 7;
/// Hue (degrees 0–359) for each morph node — full saturation at the node,
/// desaturates toward the next node so the waveform type is always readable.
const NODE_HUES: [u16; MORPH_NODES] = [0, 45, 90, 135, 180, 225, 270];
/// How hard Symmetry leans away from 50% for a given fader offset (< 1 = more extreme).
const SYMMETRY_LEAN_CURVE: f32 = 0.45;

pub fn symmetry_phase(phase: usize, symmetry: u16) -> usize {
    // Piecewise phase remap: center (2048) = balanced halves.
    // Lean curve pulls toward extremes faster so shape changes read clearly live.
    let t = (phase % 4096) as f32 / 4096.0;
    let centered = (symmetry as f32 / 4095.0 - 0.5) * 2.0; // -1..1
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
    // Smoothstep blend toward ease-in/out time feel
    let eased = t * t * (3.0 - 2.0 * t);
    let out = t * (1.0 - amount) + eased * amount;
    (out * 4095.0) as usize
}

pub fn skew_phase(phase: usize, skew: u16) -> usize {
    // Center (2048) = linear; low/high lean soft asymmetry (pow curve)
    let t = (phase % 4096) as f32 / 4096.0;
    let s = (skew as f32 / 4095.0 - 0.5) * 2.0; // -1..1
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
        // Gentle drift (~±3 at 1 kHz audio tick).
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
            // S&H — new level every 1/16 of the cycle (phase bucket).
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

/// `form` = (skew, warp, symmetry). `osc` selects which chaos state a stepped
/// node draws from, so two oscillators stay decorrelated.
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
    // Past the last node (morph 4092–4095 due to 4096/6 remainder): pure Noise.
    if raw_seg >= segments {
        return node_sample(MORPH_NODES - 1, p, osc, chaos, die);
    }
    let frac = (morph as usize) % seg_size;
    let a = node_sample(raw_seg, p, osc, chaos, die) as i32;
    let b = node_sample(raw_seg + 1, p, osc, chaos, die) as i32;
    (a + (b - a) * frac as i32 / seg_size as i32).clamp(0, 4095) as u16
}

/// Node-snap morph color: full saturation at each waveform anchor, linearly
/// desaturates toward the next node. The new hue snaps in at full saturation
/// the moment the fader crosses a node boundary. The last node (Noise) has no
/// successor, so its segment stays at full blue saturation throughout.
pub fn morph_color(morph: u16) -> Color {
    let segments = MORPH_NODES - 1; // 6
    let seg_size = 4096 / segments; // 682
    let m = morph.min(4095) as usize;
    let seg = (m / seg_size).min(segments - 1);
    let frac = m % seg_size;
    // Last segment (S&H → Noise): no next node, so Noise blue throughout.
    if seg == segments - 1 {
        let (r, g, b) = hsv_to_rgb(NODE_HUES[MORPH_NODES - 1]);
        return Color::Custom(r, g, b);
    }
    let sat = 255u8.saturating_sub((frac * 255 / seg_size) as u8);
    let (r, g, b) = hsv_to_rgb_sat(NODE_HUES[seg], sat);
    Color::Custom(r, g, b)
}

/// Integer HSV→RGB with V=max and variable saturation (0 = white, 255 = full hue).
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

/// Integer HSV→RGB with S=V=max. Hue in degrees (0..360).
fn hsv_to_rgb(hue: u16) -> (u8, u8, u8) {
    let sector = hue / 60; // 0..=5
                           // Rising/falling ramp within the sector, scaled to 0..=255.
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
