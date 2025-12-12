// Todo
// Quantizer
//clock res

use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    constants::BJORKLUND_PATTERNS, ext::FromValue, latch::LatchLayer, AppIcon, Brightness,
    ClockDivision, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 3;
pub const PARAMS: usize = 4;

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<PARAMS> = Config::new(
    "GenSeq",
    "Turing machine, synched to internal clock",
    Color::Blue,
    AppIcon::SequenceSquare,
)
.add_param(Param::i32 {
    //is it possible to have this apear only if CC or note are selected
    name: "Midi channel",
    min: 1,
    max: 16,
})
.add_param(Param::i32 {
    name: "Base Note",
    min: 1,
    max: 128,
})
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
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
});

pub struct Params {
    midi_channel: i32,
    note: i32,
    gatel: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: 1,
            note: 36,
            gatel: 50,
            color: Color::Blue,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: i32::from_value(values[0]),
            note: i32::from_value(values[1]),
            gatel: i32::from_value(values[2]),
            color: Color::from_value(values[3]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    att_saved: u16,
    length_saved: u16,
    register_pitch: u16,
    register_rhythm_length: u16,
    register_rhythm_beat: u16,
    res_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            att_saved: 3000,
            length_saved: 8,
            register_pitch: 0,
            register_rhythm_length: 0,
            register_rhythm_beat: 0,
            res_saved: 2048,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
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
    let (led_color, midi_chan, base_note, gatel) =
        params.query(|p| (p.color, p.midi_channel, p.note, p.gatel));
    let range = Range::_0_10V;

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let die = app.use_die();
    let quantizer = app.use_quantizer(range);

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let prob_glob = app.make_global(0);

    let recall_flag = app.make_global(false);
    let div_glob = app.make_global(4);
    let midi_note = app.make_global(0);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let latched_glob = app.make_global(true);

    let resolution = [24, 16, 12, 8, 6, 4, 3, 2];

    leds.set(0, Led::Button, led_color, Brightness::Lower);

    let cv_out = app.make_out_jack(0, Range::_0_10V).await;
    let gate_out = app.make_out_jack(1, Range::_0_10V).await;

    let curve = Curve::Exponential;

    let (mut register_pitch, mut register_length, mut register_beat) = storage.query(|s| {
        (
            s.register_pitch,
            s.register_rhythm_length,
            s.register_rhythm_beat,
        )
    });
    let res = storage.query(|s| (s.res_saved));

    div_glob.set(resolution[res as usize / 512]);

    let fut1 = async {
        let mut clkn: u32 = 0;
        let mut att_reg = 0;
        let mut gate_on = false;
        let mut clkn_euclid: u16 = 0;
        let mut euclid_lenght = 7;
        let mut euclid_beat = 3;

        loop {
            let length = storage.query(|s| (s.length_saved));
            let div = div_glob.get();
            let mut note = 0;

            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    midi.send_note_off((att_reg / 32) as u8).await;
                    register_pitch = storage.query(|s| (s.register_pitch));
                }
                ClockEvent::Tick => {
                    if clkn % div == 0 {
                        clkn_euclid += 1;
                        clkn_euclid = clkn_euclid % euclid_lenght as u16;

                        if euclidean_filter(euclid_lenght, euclid_beat, 0, clkn_euclid as u32) {
                            gate_out.set_value(4095);
                            gate_on = true;
                        }
                        if clkn_euclid == 0 {
                            let prob = prob_glob.get();
                            let rand = die.roll().clamp(100, 3900);

                            let rotation = rotate_select_bit(register_pitch, prob, rand, length);
                            register_pitch = rotation.0;

                            let register_scalled = scale_to_12bit(register_pitch, length as u8);
                            att_reg = ((register_scalled as u32
                                * curve.at(storage.query(|s| (s.att_saved))) as u32)
                                / 4095) as u16;

                            let out = quantizer.get_quantized_note(att_reg).await;

                            cv_out.set_value(out.as_counts(Range::_0_10V));
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom((register_scalled / 16) as u8),
                            );
                            // info!("{}", register_scalled);

                            let note = out.as_midi() + base_note as u8;
                            midi.send_note_on(note, 4095).await;

                            midi_note.set(note);

                            if buttons.is_button_pressed(0) && !buttons.is_shift_pressed() {
                                leds.set(0, Led::Bottom, Color::Red, Brightness::Low);
                            }
                        }
                    }
                    // note off
                    if clkn % div == (div * gatel as u32 / 100).clamp(1, div - 1) {
                        leds.unset(0, Led::Bottom);

                        let note = midi_note.get();
                        midi.send_note_off(note).await;
                        if gate_on {
                            gate_out.set_value(0);
                            gate_on = false;
                        }
                    }

                    if (clkn / div) % length as u32 == 0 {
                        let reg_old = storage.query(|s| (s.register_pitch));
                        if recall_flag.get() {
                            register_pitch = reg_old;
                            recall_flag.set(false);
                            midi.send_note_off(note).await;
                        }

                        if register_pitch != reg_old {
                            storage.modify_and_save(|s| s.register_pitch = register_pitch);
                        }
                    }

                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        let mut latch = [
            app.make_latch(fader.get_value_at(0)),
            app.make_latch(fader.get_value_at(1)),
            app.make_latch(fader.get_value_at(2)),
        ];

        loop {
            let chan = fader.wait_for_any_change().await;
            if chan == 0 {
                let latch_layer = glob_latch_layer.get();

                let target_value = match latch_layer {
                    LatchLayer::Main => prob_glob.get(),
                    LatchLayer::Alt => storage.query(|s| s.att_saved),
                    LatchLayer::Third => storage.query(|s| s.res_saved),
                };

                if let Some(new_value) =
                    latch[chan].update(fader.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            prob_glob.set(new_value);
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| s.att_saved = new_value);
                        }
                        LatchLayer::Third => {
                            div_glob.set(resolution[new_value as usize / 512]);
                            let note = midi_note.get();
                            midi.send_note_off(note).await;
                            storage.modify_and_save(|s| s.res_saved = new_value);
                        }
                    }
                }
            }
        }
    };

    let rec_flag = app.make_global(false);
    let length_rec = app.make_global(0);

    let fut3 = async {
        loop {
            let shift = buttons.wait_for_down(0).await;
            // latched_glob.set(false);
            let mut length = length_rec.get();
            if shift && rec_flag.get() {
                length += 1;
                length_rec.set(length.min(16));
            }
        }
    };

    let fut4 = async {
        let mut shift_old = false;
        let mut button_old = false;
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

            if buttons.is_shift_pressed() {
                if !shift_old {
                    // latched_glob.set(false);
                    shift_old = true;
                    rec_flag.set(true);
                    length_rec.set(0);
                }
                leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((storage.query(|s| (s.att_saved)) / 16) as u8),
                );
            }
            if !buttons.is_shift_pressed() && shift_old {
                // latched_glob.set(false);
                shift_old = false;
                rec_flag.set(false);
                let length = length_rec.get();
                if length >= 1 {
                    storage.modify_and_save(|s| s.length_saved = length);
                }
            }

            if buttons.is_button_pressed(0) {
                //button going down
                if !button_old {
                    // latched_glob.set(false);
                    button_old = true;
                }
            }
            if !buttons.is_button_pressed(0) && button_old {
                // latched_glob.set(false);
                button_old = false;
                leds.unset(0, Led::Bottom);
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load_from_scene(scene).await;
                    let res = storage.query(|s| (s.res_saved));

                    recall_flag.set(true);
                    prob_glob.set(0);
                    div_glob.set(resolution[res as usize / 512]);

                    //Add recall routine
                    latched_glob.set(false);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, scene_handler).await;
}
///Returns rotated register and of the bit had been flipped
fn rotate_select_bit(x: u16, a: u16, b: u16, bit_index: u16) -> (u16, bool) {
    let bit_index = (16 - bit_index).clamp(0, 16);

    // Extract the original bit
    let original_bit = ((x >> bit_index) & 1) as u8;
    let mut bit = original_bit;

    // Invert the bit if a > b
    if a > b {
        bit ^= 1;
    }

    // Shift x right by 1
    let shifted = x >> 1;

    // Insert the (possibly inverted) bit into the MSB
    let result = shifted | ((bit as u16) << 15);

    // Return the new value and whether the bit was flipped
    let flipped = bit != original_bit;
    (result, flipped)
}

fn scale_to_12bit(input: u16, x: u8) -> u16 {
    let x = x.clamp(1, 16);

    // Shift to keep the top `x` bits
    let top_x_bits = input >> (16 - x);

    // Scale to 12-bit
    let max_x_val = (1 << x) - 1;
    ((top_x_bits as u32 * 4095) / max_x_val as u32) as u16
}

/// Rotate left a u32 pattern within a given bit width
fn rotl32(value: u32, width: u8, rotation: u8) -> u32 {
    let rotation = rotation % width;
    ((value << rotation) | (value >> (width - rotation))) & ((1 << width) - 1)
}

/// Get the Euclidean pattern as a u32
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

/// Check if there's a beat at a given clock position
fn euclidean_filter(num_steps: u8, num_beats: u8, rotation: u8, clock: u32) -> bool {
    let pattern = euclidean_pattern(num_steps, num_beats, rotation, 0);
    let pos = (clock % num_steps as u32) as u8;
    (pattern & (1 << pos)) != 0
}
