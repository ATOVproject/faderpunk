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
    quantizer::Pitch,
    utils::{attenuate, euclidean_at, rotate_select_bit, scale_to_12bit},
    AppIcon, Brightness, ClockDivision, Color, Config, Curve, MidiChannel, MidiNote, MidiOut,
    Param, Range, Value, VoltPerOct, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 6;

/// LED colors for the 5 octave shift steps -2..+2 (F1 Third layer)
const OCT_COLORS: [Color; 5] = [
    Color::Blue,
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Red,
];

pub static CONFIG: Config<PARAMS> = Config::new(
    "GenSeq",
    "Generative sequencer with Turing machine registers",
    Color::Yellow,
    AppIcon::Sequence,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "Base Note" })
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
.add_param(Param::MidiOut)
.add_param(Param::VoltPerOct)
.add_param(Param::bool {
    name: "Bypass quantizer",
});

pub struct Params {
    midi_channel: MidiChannel,
    note: MidiNote,
    color: Color,
    midi_out: MidiOut,
    vpo: VoltPerOct,
    bypass: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            note: MidiNote::from(12),
            color: Color::Blue,
            midi_out: MidiOut::default(),
            vpo: VoltPerOct::Standard,
            bypass: false,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            note: MidiNote::from_value(values[1]),
            color: Color::from_value(values[2]),
            midi_out: MidiOut::from_value(values[3]),
            vpo: VoltPerOct::from_value(values[4]),
            bypass: bool::from_value(values[5]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec.push(self.vpo.into()).unwrap();
        vec.push(self.bypass.into()).unwrap();
        vec
    }
}

/// Fader layout:
///   F0 Main=pitch_att     Alt=octave_shift    Third=res_saved
///   F1 Main=length_att    Alt=legato_att      Third=(none)
///   F2 Main=beat_density  Alt=accent_att      Third=gate_length
///
/// Buttons (no shift): hold to mutate that register
///   Btn0=pitch  Btn1=length
///
/// Shift+Button counting (while shift held, each press increments; release commits):
///   Shift+Btn0: pitch TM length (1–16) → pitch_tm_length
///   Shift+Btn1: length TM register width (1–16) → length_tm_length
///
/// Outputs:
///   Jack0=CV (quantized pitch)  Jack1=Gate  Jack2=Accent CV (4095 when accented)
#[derive(Serialize, Deserialize)]
pub struct Storage {
    // Main layer
    pitch_att: u16,
    length_att: u16,
    beat_density: u16,
    // Alt layer
    legato_att: u16,
    accent_att: u16,
    // Third layer
    res_saved: u16,
    octave_shift: u16,
    gate_length: u16,
    // Turing machine state
    register_pitch: u16,
    register_length: u16,
    // TM register widths (1–16, set by Shift+Button counting)
    pitch_tm_length: u8,
    length_tm_length: u8,
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            pitch_att: 3000,
            length_att: 2048,
            beat_density: 2048,
            legato_att: 1024,
            accent_att: 1024,
            res_saved: 2048,    // resolution[4] = 6 PPQN
            octave_shift: 1638, // 1638/819=2 → 2-2=0 octave shift
            gate_length: 2048,  // ~50%
            register_pitch: 7632,
            register_length: 534,
            pitch_tm_length: 9,
            length_tm_length: 16,
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

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (led_color, midi_chan, base_note, midi_out, vpo, bypass) =
        params.query(|p| (p.color, p.midi_channel, p.note, p.midi_out, p.vpo, p.bypass));

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let ticks = clock.get_ticker();
    let die = app.use_die();
    let quantizer = app.use_quantizer(Range::_0_10V, vpo, bypass);
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let prob_pitch_glob = app.make_global(0u16);
    let prob_length_glob = app.make_global(0u16);
    let recall_flag = app.make_global(false);
    let div_glob = app.make_global(4u32);
    let midi_note = app.make_global(MidiNote::from(0));
    let last_note_on = app.make_global(MidiNote::from(0));
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let glob_muted = app.make_global(storage.query(|s| s.muted));
    let third_layer_used = app.make_global(false);
    let pitch_tm_step_glob = app.make_global(0u8);
    let length_tm_step_glob = app.make_global(0u8);
    let euclid_step_glob = app.make_global(0u16);
    let euclid_len_glob = app.make_global(1u8);

    let resolution = [24u32, 16, 12, 8, 6, 4, 3, 2];

    leds.set(0, Led::Button, led_color, Brightness::Low);
    leds.set(1, Led::Button, led_color, Brightness::Low);
    if storage.query(|s| s.muted) {
        leds.unset(2, Led::Button);
    } else {
        leds.set(2, Led::Button, led_color, Brightness::Low);
    }

    let cv_out = app.make_out_jack(0, Range::_0_10V).await;
    let gate_out = app.make_out_jack(1, Range::_0_10V).await;
    let aux_out = app.make_out_jack(2, Range::_0_10V).await;

    let (mut register_pitch, mut register_length) =
        storage.query(|s| (s.register_pitch, s.register_length));

    let res = storage.query(|s| s.res_saved);
    div_glob.set(resolution[(res / 512).min(7) as usize]);

    // Clock-driven sequencer loop
    let fut1 = async {
        let mut gate_on = false;
        let mut clkn_euclid: u16 = 0;
        let mut beat_reg_length: u8 = storage.query(|s| s.length_tm_length).clamp(1, 16);
        let mut euclid_length: u8 = attenuate(
            length_register_scaled(register_length, beat_reg_length),
            storage.query(|s| s.length_att),
        )
        .max(1) as u8;
        let mut euclid_beat: u8 = (storage.query(|s| s.beat_density) as u32 * euclid_length as u32
            / 4095)
            .clamp(1, euclid_length as u32) as u8;
        let mut pending_note_off = false;
        let mut legato = false;
        let mut legato_register: u16 = register_pitch ^ register_length;
        let mut accent_register: u16 = register_pitch ^ register_length.rotate_right(8);
        let mut pitch_cycle_step: u8 = 0;
        let mut length_cycle_step: u8 = 0;

        loop {
            let div = div_glob.get();
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn_euclid = 0;
                    pitch_cycle_step = 0;
                    length_cycle_step = 0;
                    pitch_tm_step_glob.set(0);
                    length_tm_step_glob.set(0);
                    euclid_step_glob.set(0);
                    if gate_on {
                        gate_out.set_value(0);
                        midi.send_note_off(last_note_on.get()).await;
                        gate_on = false;
                    }
                    register_pitch = storage.query(|s| s.register_pitch);
                }
                ClockEvent::Tick => {
                    let clkn = ticks() as u32;
                    if clkn.is_multiple_of(div) {
                        clkn_euclid = (clkn_euclid + 1) % euclid_length.max(1) as u16;
                        euclid_step_glob.set(clkn_euclid);

                        if clkn_euclid == 0 {
                            // Cycle boundary: rotate TMs, recompute pattern params and pitch
                            let prob_pitch = prob_pitch_glob.get();
                            let prob_length = prob_length_glob.get();

                            beat_reg_length = storage.query(|s| s.length_tm_length).clamp(1, 16);

                            let rand = die.roll().clamp(100, 3900);
                            register_length = rotate_select_bit(
                                register_length,
                                prob_length,
                                rand,
                                beat_reg_length as u16,
                            )
                            .0;

                            euclid_length = attenuate(
                                length_register_scaled(register_length, beat_reg_length),
                                storage.query(|s| s.length_att),
                            )
                            .max(1) as u8;
                            euclid_len_glob.set(euclid_length);

                            euclid_beat = (storage.query(|s| s.beat_density) as u32
                                * euclid_length as u32
                                / 4095)
                                .clamp(1, euclid_length as u32)
                                as u8;

                            let length = storage.query(|s| s.pitch_tm_length).clamp(1, 16) as u16;
                            let rand = die.roll().clamp(100, 3900);
                            register_pitch =
                                rotate_select_bit(register_pitch, prob_pitch, rand, length).0;

                            // Advance TM cycle step counters for Bottom LED progress display
                            let ptm_len = length as u8;
                            pitch_cycle_step = (pitch_cycle_step + 1) % ptm_len;
                            pitch_tm_step_glob.set(pitch_cycle_step);
                            let ltm_len = beat_reg_length;
                            length_cycle_step = (length_cycle_step + 1) % ltm_len;
                            length_tm_step_glob.set(length_cycle_step);

                            let register_scaled = scale_to_12bit(register_pitch, length as u8);
                            let raw_att = storage.query(|s| s.pitch_att);
                            let att_reg = attenuate(register_scaled, raw_att);
                            let octave_offset =
                                (storage.query(|s| s.octave_shift) / 819).min(4) as i32 - 2;

                            // When shifting down, raise the quantizer input floor so the
                            // bottom of the pitch range maps to C0 rather than clipping
                            // negative DAC values to 0V.
                            let input_floor = ((-octave_offset).max(0) as u16)
                                .saturating_mul(vpo.counts_per_oct() as u16);
                            let quantizer_input =
                                (att_reg as u32 / 2 + input_floor as u32).min(4095) as u16;
                            let out = quantizer.get_quantized_note(quantizer_input).await;

                            // Apply the octave shift in pitch space to avoid the ±1 count
                            // rounding error from the integer counts_per_oct approximation.
                            let shifted = Pitch {
                                octave: (out.octave as i32 + octave_offset) as i8,
                                note: out.note,
                                raw: None,
                            };

                            let base_note_i = u7::from(base_note).as_int() as i32;
                            let shifted_midi_val = u7::from(shifted.as_midi()).as_int() as i32;
                            let note =
                                MidiNote::from((base_note_i + shifted_midi_val - 12).clamp(0, 127));
                            midi_note.set(note);

                            if !glob_muted.get() {
                                cv_out.set_value(shifted.as_counts(Range::_0_10V, vpo));
                            }
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom((register_scaled / 16) as u8),
                            );

                            let reg_old = storage.query(|s| s.register_pitch);
                            let reg_len_old = storage.query(|s| s.register_length);
                            if recall_flag.get() {
                                register_pitch = reg_old;
                                register_length = reg_len_old;
                                euclid_length = attenuate(
                                    length_register_scaled(register_length, beat_reg_length),
                                    storage.query(|s| s.length_att),
                                )
                                .max(1) as u8;
                                euclid_len_glob.set(euclid_length);
                                recall_flag.set(false);
                            } else {
                                if register_pitch != reg_old {
                                    storage.modify_and_save(|s| s.register_pitch = register_pitch);
                                }
                                if register_length != reg_len_old {
                                    storage
                                        .modify_and_save(|s| s.register_length = register_length);
                                }
                            }

                            // Derive legato/accent registers from XOR of both TMs
                            legato_register = register_pitch ^ register_length;
                            accent_register = register_pitch ^ register_length.rotate_right(8);
                        } else {
                            // Every other step: rotate derived registers
                            legato_register = legato_register.rotate_right(1);
                            accent_register = accent_register.rotate_right(1);
                        }

                        let is_legato =
                            scale_to_12bit(legato_register, 16) < storage.query(|s| s.legato_att);
                        let is_accented = scale_to_12bit(accent_register, 16)
                            < Curve::Exponential.at(storage.query(|s| s.accent_att));

                        let is_beat =
                            euclidean_at(euclid_length, euclid_beat, 0, clkn_euclid as u32);

                        if is_beat && !glob_muted.get() {
                            if gate_on {
                                midi.send_note_off(last_note_on.get()).await;
                            }
                            let note = midi_note.get();
                            last_note_on.set(note);
                            gate_out.set_value(4095);
                            gate_on = true;
                            pending_note_off = false;
                            aux_out.set_value(if is_accented { 4095 } else { 0 });
                            midi.send_note_on(note, if is_accented { 4095 } else { 2048 })
                                .await;
                            leds.set(1, Led::Top, led_color, Brightness::Low);
                        }

                        legato = is_legato;
                        if is_legato {
                            leds.set(2, Led::Top, led_color, Brightness::Low);
                        } else {
                            leds.set(2, Led::Top, led_color, Brightness::Custom(0));
                            if pending_note_off && gate_on {
                                gate_out.set_value(0);
                                gate_on = false;
                                pending_note_off = false;
                                leds.set(1, Led::Top, led_color, Brightness::Custom(0));
                                midi.send_note_off(last_note_on.get()).await;
                            }
                        }
                    }

                    // Resolution flash on Ch0 Bottom while in Third layer
                    if matches!(glob_latch_layer.get(), LatchLayer::Third) {
                        if clkn.is_multiple_of(div) {
                            let color = if matches!(div, 2 | 4 | 8 | 16) {
                                Color::Orange
                            } else {
                                Color::Blue
                            };
                            leds.set(0, Led::Bottom, color, Brightness::High);
                        } else if clkn % div == (div / 2).max(1) {
                            leds.unset(0, Led::Bottom);
                        }
                    }

                    // Gate off
                    let gate_pct =
                        (storage.query(|s| s.gate_length) as u32 * 100 / 4095).clamp(1, 99);
                    if clkn % div == (div * gate_pct / 100).clamp(1, div - 1) {
                        if gate_on && !legato {
                            gate_out.set_value(0);
                            gate_on = false;
                            leds.set(1, Led::Top, led_color, Brightness::Custom(0));
                            midi.send_note_off(last_note_on.get()).await;
                        } else if gate_on && legato {
                            pending_note_off = true;
                        }
                    }

                }
                _ => {}
            }
        }
    };

    // Fader handler
    let fut2 = async {
        let mut latch = [
            app.make_latch(fader.get_value_at(0)),
            app.make_latch(fader.get_value_at(1)),
            app.make_latch(fader.get_value_at(2)),
        ];

        loop {
            let chan = fader.wait_for_any_change().await;
            let layer = glob_latch_layer.get();
            if matches!(layer, LatchLayer::Third) {
                third_layer_used.set(true);
            }

            let target = match (chan, layer) {
                (0, LatchLayer::Main) => storage.query(|s| s.pitch_att),
                (0, LatchLayer::Alt) => storage.query(|s| s.octave_shift),
                (0, LatchLayer::Third) => storage.query(|s| s.res_saved),
                (1, LatchLayer::Main) => storage.query(|s| s.length_att),
                (1, LatchLayer::Alt) => storage.query(|s| s.legato_att),
                (2, LatchLayer::Main) => storage.query(|s| s.beat_density),
                (2, LatchLayer::Alt) => storage.query(|s| s.accent_att),
                (2, LatchLayer::Third) => storage.query(|s| s.gate_length),
                _ => 0,
            };

            if let Some(v) = latch[chan].update(fader.get_value_at(chan), layer, target) {
                match (chan, layer) {
                    (0, LatchLayer::Main) => storage.modify_and_save(|s| s.pitch_att = v),
                    (0, LatchLayer::Alt) => storage.modify_and_save(|s| s.octave_shift = v),
                    (0, LatchLayer::Third) => {
                        div_glob.set(resolution[(v / 512).min(7) as usize]);
                        midi.send_note_off(last_note_on.get()).await;
                        storage.modify_and_save(|s| s.res_saved = v);
                    }
                    (1, LatchLayer::Main) => storage.modify_and_save(|s| s.length_att = v),
                    (1, LatchLayer::Alt) => storage.modify_and_save(|s| s.legato_att = v),
                    (2, LatchLayer::Main) => storage.modify_and_save(|s| s.beat_density = v),
                    (2, LatchLayer::Alt) => storage.modify_and_save(|s| s.accent_att = v),
                    (2, LatchLayer::Third) => storage.modify_and_save(|s| s.gate_length = v),
                    _ => {}
                }
            }
        }
    };

    // Layer management, mutation, TM length counting, mute, and LED feedback
    let fut3 = async {
        let mut shift_old = false;
        let mut btn0_old = false;
        let mut btn1_old = false;
        let mut btn2_old = false;
        let mut pitch_len_count: u8 = 0;
        let mut length_len_count: u8 = 0;
        let mut was_overlay = false;

        loop {
            app.delay_millis(1).await;

            let shift = buttons.is_shift_pressed();
            let btn0 = buttons.is_button_pressed(0);
            let btn1 = buttons.is_button_pressed(1);
            let btn2 = buttons.is_button_pressed(2);

            // Btn2 rising edge: reset Third-layer-used flag
            if btn2 && !btn2_old {
                third_layer_used.set(false);
            }
            // Mute toggle: Btn2 release, only if no Third layer fader was moved
            if !btn2 && btn2_old && !shift && !third_layer_used.get() {
                let muted = !glob_muted.get();
                glob_muted.set(muted);
                storage.modify_and_save(|s| s.muted = muted);
            }
            btn2_old = btn2;

            // TM length counting: Shift + Btn0/Btn1 presses (edge-triggered)
            let was_shift = shift_old;
            shift_old = shift;

            // Reset at the start of a new Shift session, before counting
            if shift && !was_shift {
                pitch_len_count = 0;
                length_len_count = 0;
            }

            // Count presses (safe now — reset already fired this tick if needed)
            if shift {
                if btn0 && !btn0_old {
                    pitch_len_count = pitch_len_count.saturating_add(1).min(16);
                }
                if btn1 && !btn1_old {
                    length_len_count = length_len_count.saturating_add(1).min(16);
                }
            }
            btn0_old = btn0;
            btn1_old = btn1;

            // Commit on Shift release
            if !shift && was_shift {
                if pitch_len_count >= 1 {
                    storage.modify_and_save(|s| s.pitch_tm_length = pitch_len_count);
                }
                if length_len_count >= 1 {
                    storage.modify_and_save(|s| s.length_tm_length = length_len_count);
                }
                pitch_len_count = 0;
                length_len_count = 0;
            }

            let layer = if shift {
                LatchLayer::Alt
            } else if btn2 {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(layer);

            if !shift {
                prob_pitch_glob.set(if btn0 { 2048 } else { 0 });
                prob_length_glob.set(if btn1 { 2048 } else { 0 });
            } else {
                prob_pitch_glob.set(0);
                prob_length_glob.set(0);
            }

            let muted = glob_muted.get();
            let is_overlay = !matches!(layer, LatchLayer::Main);

            // Returning to Main: clear overlay LEDs so fut1 takes over cleanly
            if was_overlay && !is_overlay {
                leds.unset(0, Led::Top);
                leds.unset(1, Led::Top);
                leds.unset(2, Led::Top);
                leds.unset(0, Led::Bottom);
                leds.unset(1, Led::Bottom);
                leds.unset(2, Led::Bottom);
            }
            was_overlay = is_overlay;

            match layer {
                LatchLayer::Main => {
                    // Btn0/Btn1: brighter when held (mutating)
                    leds.set(
                        0,
                        Led::Button,
                        led_color,
                        if btn0 {
                            Brightness::High
                        } else {
                            Brightness::Low
                        },
                    );
                    leds.set(
                        1,
                        Led::Button,
                        led_color,
                        if btn1 {
                            Brightness::High
                        } else {
                            Brightness::Low
                        },
                    );
                    // Btn2: mute indicator — dim when active, off when muted
                    if muted {
                        leds.unset(2, Led::Button);
                    } else {
                        leds.set(2, Led::Button, led_color, Brightness::Low);
                    }
                    // Bottom LEDs: TM cycle progress (bright at step 0, dims through cycle)
                    let pitch_step = pitch_tm_step_glob.get() as u32;
                    let pitch_len = storage.query(|s| s.pitch_tm_length).max(1) as u32;
                    let p0 = (255u32.saturating_sub(pitch_step * 255 / pitch_len)) as u8;
                    leds.set(0, Led::Bottom, led_color, Brightness::Custom(p0));
                    let len_step = length_tm_step_glob.get() as u32;
                    let len_len = storage.query(|s| s.length_tm_length).max(1) as u32;
                    let p1 = (255u32.saturating_sub(len_step * 255 / len_len)) as u8;
                    leds.set(1, Led::Bottom, led_color, Brightness::Custom(p1));
                    let e_step = euclid_step_glob.get() as u32;
                    let e_len = euclid_len_glob.get().max(1) as u32;
                    let p2 = (255u32.saturating_sub(e_step * 255 / e_len)) as u8;
                    leds.set(2, Led::Bottom, led_color, Brightness::Custom(p2));
                }
                LatchLayer::Alt => {
                    let oct_idx = (storage.query(|s| s.octave_shift) / 819).min(4) as usize;
                    // F0 Button LED: live count while tapping Shift+Btn0; octave color otherwise
                    if pitch_len_count > 0 {
                        leds.set(
                            0,
                            Led::Button,
                            Color::White,
                            Brightness::Custom((pitch_len_count as u16 * 16).min(255) as u8),
                        );
                    } else {
                        leds.set(0, Led::Button, OCT_COLORS[oct_idx], Brightness::High);
                    }
                    let len_disp = if length_len_count > 0 {
                        length_len_count
                    } else {
                        storage.query(|s| s.length_tm_length)
                    };
                    leds.set(
                        1,
                        Led::Button,
                        Color::Green,
                        Brightness::Custom((len_disp as u16 * 16).min(255) as u8),
                    );
                    if muted {
                        leds.unset(2, Led::Button);
                    } else {
                        leds.set(2, Led::Button, led_color, Brightness::Low);
                    }
                    // Top LEDs: fader values (brightness = current setting)
                    leds.set(0, Led::Top, OCT_COLORS[oct_idx], Brightness::High);
                    leds.set(
                        1,
                        Led::Top,
                        Color::Cyan,
                        Brightness::Custom((storage.query(|s| s.legato_att) / 16) as u8),
                    );
                    leds.set(
                        2,
                        Led::Top,
                        Color::Yellow,
                        Brightness::Custom((storage.query(|s| s.accent_att) / 16) as u8),
                    );
                }
                LatchLayer::Third => {
                    leds.set(
                        0,
                        Led::Button,
                        led_color,
                        if btn0 {
                            Brightness::High
                        } else {
                            Brightness::Low
                        },
                    );
                    leds.set(
                        1,
                        Led::Button,
                        led_color,
                        if btn1 {
                            Brightness::High
                        } else {
                            Brightness::Low
                        },
                    );
                    leds.set(2, Led::Button, led_color, Brightness::High);
                    leds.set(
                        2,
                        Led::Top,
                        Color::White,
                        Brightness::Custom((storage.query(|s| s.gate_length) / 16) as u8),
                    );
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let res = storage.query(|s| s.res_saved);
                    div_glob.set(resolution[(res / 512).min(7) as usize]);
                    recall_flag.set(true);
                    glob_muted.set(storage.query(|s| s.muted));
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

/// Scale the length register to the 0–16 range used for euclid_length.
/// When bit_length == 1, bypasses the register and returns 16 (full scale) so
/// length_att becomes a direct euclidean length control.
fn length_register_scaled(register: u16, bit_length: u8) -> u16 {
    if bit_length == 1 {
        16
    } else {
        (scale_to_12bit(register, bit_length) as u32 * 16 / 4095) as u16
    }
}
