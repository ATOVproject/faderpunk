use embassy_futures::{
    join::join4,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    constants::BJORKLUND_PATTERNS,
    ext::FromValue,
    latch::LatchLayer,
    utils::attenuate,
    AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, Param,
    Range, Value, APP_MAX_PARAMS,
};
use midly::num::u7;

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 4;

/// LED colors for the 8 clock resolution steps (F0 Third layer)
const RES_COLORS: [Color; 8] = [
    Color::Red,
    Color::Orange,
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::Violet,
    Color::Pink,
];

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
    Color::Blue,
    AppIcon::SequenceSquare,
)
.add_param(Param::MidiChannel { name: "MIDI Channel" })
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
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    note: MidiNote,
    color: Color,
    midi_out: MidiOut,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            note: MidiNote::from(36),
            color: Color::Blue,
            midi_out: MidiOut::default(),
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
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

/// Fader layout:
///   F0 Main=pitch_att     Alt=pitch_length_saved  Third=res_saved
///   F1 Main=length_att    Alt=legato_att          Third=octave_shift
///   F2 Main=beat_density  Alt=accent_att          Third=gate_length
///
/// Buttons (no shift): hold to mutate that register
///   Btn0=pitch  Btn1=length  Btn2=legato+accent
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
    pitch_length_saved: u16,
    legato_att: u16,
    accent_att: u16,
    // Third layer
    res_saved: u16,
    octave_shift: u16,
    gate_length: u16,
    // Turing machine state (register_pitch persists; rhythm registers are ephemeral)
    register_pitch: u16,
    register_length: u16,
    register_legato: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            pitch_att: 3000,
            length_att: 2048,
            beat_density: 2048,
            pitch_length_saved: 2048, // ~9 steps
            legato_att: 1024,
            accent_att: 1024,
            res_saved: 2048,  // resolution[4] = 6 PPQN
            octave_shift: 1638, // 1638/819=2 → 2-2=0 octave shift
            gate_length: 2048,  // ~50%
            register_pitch: 7632,
            register_length: 534,
            register_legato: 2821,
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
    let (led_color, midi_chan, base_note, midi_out) =
        params.query(|p| (p.color, p.midi_channel, p.note, p.midi_out));

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let die = app.use_die();
    let quantizer = app.use_quantizer(Range::_0_10V);
    let midi = app.use_midi_output(midi_out, midi_chan, false);

    let prob_pitch_glob = app.make_global(0u16);
    let prob_length_glob = app.make_global(0u16);
    let prob_legato_glob = app.make_global(0u16);
    let recall_flag = app.make_global(false);
    let div_glob = app.make_global(4u32);
    let midi_note = app.make_global(MidiNote::from(0));
    let last_note_on = app.make_global(MidiNote::from(0));
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let resolution = [24u32, 16, 12, 8, 6, 4, 3, 2];

    leds.set(0, Led::Button, led_color, Brightness::Low);
    leds.set(1, Led::Button, led_color, Brightness::Low);
    leds.set(2, Led::Button, led_color, Brightness::Low);

    let cv_out = app.make_out_jack(0, Range::_0_10V).await;
    let gate_out = app.make_out_jack(1, Range::_0_10V).await;
    let aux_out = app.make_out_jack(2, Range::_0_10V).await;

    let (mut register_pitch, mut register_length, mut register_legato) =
        storage.query(|s| (s.register_pitch, s.register_length, s.register_legato));

    let res = storage.query(|s| s.res_saved);
    div_glob.set(resolution[(res / 512).min(7) as usize]);

    // Clock-driven sequencer loop
    let fut1 = async {
        let mut clkn: u32 = 0;
        let mut gate_on = false;
        let mut clkn_euclid: u16 = 0;
        let mut euclid_length: u8 = 7;
        let mut euclid_beat: u8 = 3;
        let mut legato_beat: u8 = 2;
        let mut accent_beat: u8 = 2;
        let mut pending_note_off = false;
        let mut legato = false;
        let beat_reg_length: u8 = 16;

        loop {
            let div = div_glob.get();
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    if gate_on {
                        gate_out.set_value(0);
                        midi.send_note_off(last_note_on.get()).await;
                        gate_on = false;
                    }
                    register_pitch = storage.query(|s| s.register_pitch);
                }
                ClockEvent::Tick => {
                    if clkn % div == 0 {
                        clkn_euclid = (clkn_euclid + 1) % euclid_length.max(1) as u16;

                        let is_beat =
                            euclidean_filter(euclid_length, euclid_beat, 0, clkn_euclid as u32);
                        let is_legato = euclidean_filter(
                            euclid_length,
                            legato_beat,
                            (euclid_length.max(1) + 5) % euclid_length.max(1),
                            clkn_euclid as u32,
                        );
                        let is_accented =
                            euclidean_filter(euclid_length, accent_beat, 3, clkn_euclid as u32);

                        if is_beat {
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

                        // At cycle boundary: rotate TMs, recompute pattern params and pitch
                        if clkn_euclid == 0 {
                            let prob_pitch = prob_pitch_glob.get();
                            let prob_length = prob_length_glob.get();
                            let prob_legato = prob_legato_glob.get();

                            let rand = die.roll().clamp(100, 3900);
                            register_length =
                                rotate_select_bit(register_length, prob_length, rand, beat_reg_length as u16).0;

                            let rand = die.roll().clamp(100, 3900);
                            register_legato =
                                rotate_select_bit(register_legato, prob_legato, rand, beat_reg_length as u16).0;

                            euclid_length = attenuate(
                                (scale_to_12bit(register_length, beat_reg_length) as u32 * 16
                                    / 4095) as u16,
                                storage.query(|s| s.length_att),
                            )
                            .max(1) as u8;

                            // Beat density from fader: 0–100% of euclid_length
                            euclid_beat = (storage.query(|s| s.beat_density) as u32
                                * euclid_length as u32
                                / 4095)
                                .clamp(1, euclid_length as u32) as u8;

                            legato_beat = attenuate(
                                ((scale_to_12bit(register_legato, beat_reg_length)
                                    * euclid_length as u16) as u32
                                    / 2048) as u16,
                                storage.query(|s| s.legato_att),
                            )
                            .clamp(0, euclid_length as u16) as u8;

                            // Accent derived from rotated legato register bits
                            let accent_reg = register_legato.rotate_left(8);
                            accent_beat = attenuate(
                                ((scale_to_12bit(accent_reg, beat_reg_length)
                                    * euclid_length as u16) as u32
                                    / 2048) as u16,
                                storage.query(|s| s.accent_att),
                            )
                            .clamp(0, euclid_length as u16) as u8;

                            // Pitch TM rotation
                            let length = ((storage.query(|s| s.pitch_length_saved) as u32 * 16
                                / 4095)
                                + 1)
                            .min(16) as u16;
                            let rand = die.roll().clamp(100, 3900);
                            register_pitch =
                                rotate_select_bit(register_pitch, prob_pitch, rand, length).0;

                            let register_scaled = scale_to_12bit(register_pitch, length as u8);
                            let att_reg = attenuate(register_scaled, storage.query(|s| s.pitch_att));
                            let out = quantizer.get_quantized_note(att_reg / 2).await;

                            let octave_offset =
                                (storage.query(|s| s.octave_shift) / 819).min(4) as i32 - 2;
                            let base_note_i = u7::from(base_note).as_int() as i32;
                            let out_note_i = u7::from(out.as_midi()).as_int() as i32;
                            let note = MidiNote::from(
                                (base_note_i + out_note_i - 12 + octave_offset * 12).clamp(0, 127),
                            );
                            midi_note.set(note);

                            // 1V/Oct: 1 octave ≈ 410 counts in 0–10V range
                            let cv_val = (out.as_counts(Range::_0_10V) as i32
                                + octave_offset * 410)
                                .clamp(0, 4095) as u16;
                            cv_out.set_value(cv_val);
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom((register_scaled / 16) as u8),
                            );

                            // Persist pitch register at sequence boundary
                            let reg_old = storage.query(|s| s.register_pitch);
                            if recall_flag.get() {
                                register_pitch = reg_old;
                                recall_flag.set(false);
                            } else if register_pitch != reg_old {
                                storage.modify_and_save(|s| s.register_pitch = register_pitch);
                            }
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

                    clkn += 1;
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

            let target = match (chan, layer) {
                (0, LatchLayer::Main) => storage.query(|s| s.pitch_att),
                (0, LatchLayer::Alt) => storage.query(|s| s.pitch_length_saved),
                (0, LatchLayer::Third) => storage.query(|s| s.res_saved),
                (1, LatchLayer::Main) => storage.query(|s| s.length_att),
                (1, LatchLayer::Alt) => storage.query(|s| s.legato_att),
                (1, LatchLayer::Third) => storage.query(|s| s.octave_shift),
                (2, LatchLayer::Main) => storage.query(|s| s.beat_density),
                (2, LatchLayer::Alt) => storage.query(|s| s.accent_att),
                (2, LatchLayer::Third) => storage.query(|s| s.gate_length),
                _ => 0,
            };

            if let Some(v) = latch[chan].update(fader.get_value_at(chan), layer, target) {
                match (chan, layer) {
                    (0, LatchLayer::Main) => storage.modify_and_save(|s| s.pitch_att = v),
                    (0, LatchLayer::Alt) => storage.modify_and_save(|s| s.pitch_length_saved = v),
                    (0, LatchLayer::Third) => {
                        div_glob.set(resolution[(v / 512).min(7) as usize]);
                        midi.send_note_off(last_note_on.get()).await;
                        storage.modify_and_save(|s| s.res_saved = v);
                    }
                    (1, LatchLayer::Main) => storage.modify_and_save(|s| s.length_att = v),
                    (1, LatchLayer::Alt) => storage.modify_and_save(|s| s.legato_att = v),
                    (1, LatchLayer::Third) => storage.modify_and_save(|s| s.octave_shift = v),
                    (2, LatchLayer::Main) => storage.modify_and_save(|s| s.beat_density = v),
                    (2, LatchLayer::Alt) => storage.modify_and_save(|s| s.accent_att = v),
                    (2, LatchLayer::Third) => storage.modify_and_save(|s| s.gate_length = v),
                    _ => {}
                }
            }
        }
    };

    // Layer management, mutation probabilities, and LED feedback
    let fut3 = async {
        loop {
            app.delay_millis(1).await;

            let layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0) {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(layer);

            // Mutation: hold buttons (without shift) to evolve that register
            if !buttons.is_shift_pressed() {
                prob_pitch_glob.set(if buttons.is_button_pressed(0) { 2048 } else { 0 });
                prob_length_glob.set(if buttons.is_button_pressed(1) { 2048 } else { 0 });
                prob_legato_glob.set(if buttons.is_button_pressed(2) { 2048 } else { 0 });
            } else {
                prob_pitch_glob.set(0);
                prob_length_glob.set(0);
                prob_legato_glob.set(0);
            }

            match layer {
                LatchLayer::Main => {
                    leds.set(0, Led::Button, led_color, Brightness::Low);
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                    leds.set(2, Led::Button, led_color, Brightness::Low);
                }
                LatchLayer::Alt => {
                    leds.set(0, Led::Button, led_color, Brightness::Low);
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                    leds.set(2, Led::Button, led_color, Brightness::Low);
                    // Show current values as brightness
                    leds.set(
                        0,
                        Led::Top,
                        Color::White,
                        Brightness::Custom((storage.query(|s| s.pitch_length_saved) / 16) as u8),
                    );
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
                    let res_idx = (storage.query(|s| s.res_saved) / 512).min(7) as usize;
                    leds.set(0, Led::Button, RES_COLORS[res_idx], Brightness::High);
                    let oct_idx = (storage.query(|s| s.octave_shift) / 819).min(4) as usize;
                    leds.set(1, Led::Button, OCT_COLORS[oct_idx], Brightness::High);
                    leds.set(2, Led::Button, led_color, Brightness::Low);
                    leds.unset(0, Led::Top);
                    leds.unset(1, Led::Top);
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
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}

/// Rotate shift register right by 1; conditionally flip the MSB based on probability.
/// Returns (new_register, was_bit_flipped).
fn rotate_select_bit(x: u16, a: u16, b: u16, bit_index: u16) -> (u16, bool) {
    let bit_index = (16 - bit_index).clamp(0, 16);
    let original_bit = ((x >> bit_index) & 1) as u8;
    let mut bit = original_bit;
    if a > b {
        bit ^= 1;
    }
    let result = (x >> 1) | ((bit as u16) << 15);
    (result, bit != original_bit)
}

/// Extract the top `x` bits of `input` and scale linearly to 12-bit (0–4095).
fn scale_to_12bit(input: u16, x: u8) -> u16 {
    let x = x.clamp(1, 16);
    let top_x_bits = input >> (16 - x);
    let max_x_val = (1u32 << x) - 1;
    ((top_x_bits as u32 * 4095) / max_x_val) as u16
}

/// Rotate a pattern of `width` bits left by `rotation` steps.
fn rotl32(value: u32, width: u8, rotation: u8) -> u32 {
    let rotation = rotation % width;
    ((value << rotation) | (value >> (width - rotation))) & ((1 << width) - 1)
}

/// Look up the Bjorklund (Euclidean) pattern for the given step/beat counts.
fn euclidean_pattern(num_steps: u8, num_beats: u8, rotation: u8, padding: u8) -> u32 {
    let steps = num_steps.max(2);
    let beats = num_beats.min(steps);
    let index = ((steps - 2) as usize) * 33 + beats as usize;
    let mut pattern = BJORKLUND_PATTERNS.get(index).copied().unwrap_or(0);
    if rotation > 0 {
        let rot = rotation % (steps + padding);
        pattern = rotl32(pattern, steps + padding, rot);
    }
    pattern
}

/// Return true if there is a beat at `clock` position in the Euclidean pattern.
fn euclidean_filter(num_steps: u8, num_beats: u8, rotation: u8, clock: u32) -> bool {
    let pattern = euclidean_pattern(num_steps, num_beats, rotation, 0);
    let pos = (clock % num_steps.max(1) as u32) as u8;
    (pattern & (1 << pos)) != 0
}
