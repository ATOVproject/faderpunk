// Todo
// Quantizer

use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, Brightness, Color, Config, Param, Range, Value,
    APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 4;
enum Midi {
    None,
    Note,
    CC,
}

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<PARAMS> =
    Config::new("Turing+", "Classic turing machine, with clock input")
        .add_param(Param::i32 {
            //I want to be able to choose between none, CC and note
            name: "MIDI 0=off, 1=Note, 2=CC",
            min: 0,
            max: 2,
        })
        .add_param(Param::i32 {
            //is it possible to have this apear only if CC or note are selected
            name: "Midi channel",
            min: 1,
            max: 16,
        })
        .add_param(Param::i32 {
            //is it possible to have this apear only if CC or note are selected
            name: "CC number",
            min: 1,
            max: 128,
        })
        .add_param(Param::Color {
            name: "Color",
            variants: &[
                Color::Yellow,
                Color::Pink,
                Color::Cyan,
                Color::Red,
                Color::White,
            ],
        });
// .add_param(Param::i32 {
//     //is it possible to have this apear only if CC
//     name: "Scale",
//     min: 0,
//     max: 127,
// });

pub struct Params {
    midi_mode: i32,
    midi_channel: i32,
    midi_cc: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_mode: 1,
            midi_channel: 1,
            midi_cc: 1,
            color: Color::Yellow,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_mode: i32::from_value(values[0]),
            midi_channel: i32::from_value(values[1]),
            midi_cc: i32::from_value(values[2]),
            color: Color::from_value(values[3]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    att_saved: u16,
    length_saved: u16,
    register_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            att_saved: 3000,
            length_saved: 8,
            register_saved: 0,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.start_channel);

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(
                run(&app, &param_store, storage),
                param_store.param_handler(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: ManagedStorage<Storage>,
) {
    let range = Range::_0_10V;
    let (midi_mode, midi_cc, led_color, midi_chan) =
        params.query(|p| (p.midi_mode, p.midi_cc, p.color, p.midi_channel));

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let die = app.use_die();
    let midi = app.use_midi_output(midi_chan as u8 - 1);

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut att_glob = app.make_global_with_store(4095, StorageSlot::C);

    let prob_glob = app.make_global(0);
    let length_glob = app.make_global(15_u16);

    let register_glob = app.make_global(0);
    let recall_flag = app.make_global(false);
    let midi_note = app.make_global(0);

    let quantizer = app.use_quantizer(range);

    leds.set(0, Led::Button, led_color.into(), Brightness::Lower);
    leds.set(1, Led::Button, led_color.into(), Brightness::Lower);

    let input = app.make_in_jack(0, range).await;
    let output = app.make_out_jack(1, range).await;

    let base_note = 48;

    let (att, length, mut register) =
        storage.query(|s| (s.att_saved, s.length_saved, s.register_saved));

    length_glob.set(length);
    register_glob.set(register);

    let fut1 = async {
        let mut att_reg = 0;
        let mut oldinputval = 0;
        let mut note = 0;

        loop {
            app.delay_millis(1).await;
            let length = length_glob.get();

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                register = register_glob.get();
                let prob = prob_glob.get();
                let rand = die.roll().clamp(100, 3900);

                let rotation = rotate_select_bit(register, prob, rand, length);
                register = rotation.0;

                //leds.set(0, Led::Button, led_color.into(), 100 * rotation.1 as u8);

                let register_scalled = scale_to_12bit(register, length as u8);
                att_reg = (register_scalled as u32 * storage.query(|s| (s.att_saved)) as u32 / 4095)
                    as u16;

                let out = quantizer.get_quantized_note(att_reg).await;
                // let out = att_reg;

                output.set_value(out.as_counts(range));
                leds.set(
                    0,
                    Led::Top,
                    led_color.into(),
                    Brightness::Custom((register_scalled / 16) as u8),
                );
                leds.set(
                    1,
                    Led::Top,
                    led_color.into(),
                    Brightness::Custom((att_reg / 16) as u8),
                );
                // info!("{}", register_scalled);
                if midi_mode == 1 {
                    let note = out.as_midi();
                    midi.send_note_on(note, 4095).await;

                    midi_note.set(note);
                }
                if midi_mode == 2 {
                    midi.send_cc(midi_cc as u8 - 1, att_reg).await;
                }

                leds.set(0, Led::Bottom, Color::Red, Brightness::Low);
            }

            if inputval <= 406 && oldinputval > 406 {
                leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(0));

                if midi_mode == 1 {
                    let note = midi_note.get();
                    midi.send_note_off(note).await;
                }
                register_glob.set(register);
            }
            oldinputval = inputval;
        }
    };

    let fut2 = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];
        // faders handling
        loop {
            let chan = faders.wait_for_any_change().await;

            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_all_values();

            if chan == 0 {
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), LatchLayer::Main, prob_glob.get())
                {
                    prob_glob.set(new_value);
                }
            }

            if chan == 1 {
                let target_value = storage.query(|s| s.att_saved);

                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), LatchLayer::Main, target_value)
                {
                    storage
                        .modify_and_save(
                            |s| {
                                s.att_saved = new_value;
                            },
                            None,
                        )
                        .await;
                }
            }
            // let val = faders.get_all_values();
            // let att = storage.query(|s| (s.att_saved));
            // let prob = prob_glob.get();

            // let target_value = match latch_layer {
            //     LatchLayer::Main => storage.query(|s| s.att_saved[chan]),
            //     LatchLayer::Alt => storage.query(|s| s.att_saved),
            //     _ => unreachable!(),
            // };

            // if chan == 1 {
            //     if is_close(att as u16, val[chan]) {
            //         latched_glob.set(true);
            //     }

            //     if latched_glob.get() {
            //         storage
            //             .modify_and_save(|s| s.att_saved = val[chan], None)
            //             .await;
            //     }
            // }
            // if chan == 0 {
            //     prob_glob.set(val[chan]);
            // }
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

        loop {
            app.delay_millis(1).await;

            if buttons.is_shift_pressed() {
                if !shift_old {
                    shift_old = true;
                    rec_flag.set(true);
                    length_rec.set(0);
                }
            }
            if !buttons.is_shift_pressed() && shift_old {
                shift_old = false;
                rec_flag.set(false);
                let length = length_rec.get();
                if length > 1 {
                    length_glob.set(length - 1);

                    storage
                        .modify_and_save(|s| s.length_saved = length, None)
                        .await;
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (length, register) = storage.query(|s| (s.length_saved, s.register_saved));

                    length_glob.set(length);
                    register_glob.set(register);
                    recall_flag.set(true);
                    prob_glob.set(0);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, scene_handler).await;
}

fn rotate_select_bit(x: u16, a: u16, b: u16, bit_index: u16) -> (u16, bool) {
    let bit_index = (15 - bit_index).clamp(0, 15);

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
