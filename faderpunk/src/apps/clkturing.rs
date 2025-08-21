// Todo
// Quantizer

use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};
use smart_leds::colors::RED;

use libfp::{
    constants::{ATOV_BLUE, ATOV_RED, LED_HIGH, LED_MID},
    quantizer::{Key, Note},
    utils::is_close,
    Color, Config, Param, Range, Value,
};

use crate::app::{App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent, RGB8};

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
                Color::Purple,
                Color::Blue,
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

pub struct Params<'a> {
    midi_mode: ParamSlot<'a, i32, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    midi_cc: ParamSlot<'a, i32, PARAMS>,
    color: ParamSlot<'a, Color, PARAMS>,
}

// const led_color.into(): RGB8 = ATOV_BLUE;

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
    let param_store = ParamStore::new(
        [
            Value::i32(1),
            Value::i32(1),
            Value::i32(1),
            Value::Color(Color::Yellow),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_mode: ParamSlot::new(&param_store, 0),
        midi_channel: ParamSlot::new(&param_store, 1),
        midi_cc: ParamSlot::new(&param_store, 2),
        color: ParamSlot::new(&param_store, 3),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            param_store.load().await;
            storage.load(None).await;
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let die = app.use_die();

    let midi_mode = params.midi_mode.get().await;
    let midi_cc = params.midi_cc.get().await;

    let led_color = params.color.get().await;

    let midi_chan = params.midi_channel.get().await;
    let midi = app.use_midi_output(midi_chan as u8 - 1);

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut att_glob = app.make_global_with_store(4095, StorageSlot::C);

    let prob_glob = app.make_global(0);
    let length_glob = app.make_global(15_u16);
    let att_glob = app.make_global(4095);
    let register_glob = app.make_global(0);
    let recall_flag = app.make_global(false);
    let midi_note = app.make_global(0);

    let mut quantizer = app.use_quantizer();

    //Fix get this from global setting
    quantizer.set_scale(Key::Chromatic, Note::C, Note::C);

    let latched_glob = app.make_global(true);

    leds.set(0, Led::Button, led_color.into(), LED_MID);
    leds.set(1, Led::Button, led_color.into(), LED_MID);

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let base_note = 48;

    let (att, length, mut register) = storage
        .query(|s| (s.att_saved, s.length_saved, s.register_saved))
        .await;
    att_glob.set(att as u32).await;
    length_glob.set(length).await;
    register_glob.set(register).await;

    let fut1 = async {
        let mut att_reg = 0;
        let mut oldinputval = 0;
        let mut note = 0;

        loop {
            app.delay_millis(1).await;
            let length = length_glob.get().await;

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                register = register_glob.get().await;
                let prob = prob_glob.get().await;
                let rand = die.roll().clamp(100, 3900);

                let rotation = rotate_select_bit(register, prob, rand, length);
                register = rotation.0;

                //leds.set(0, Led::Button, led_color.into(), 100 * rotation.1 as u8);

                let register_scalled = scale_to_12bit(register, length as u8);
                att_reg = (register_scalled as u32 * att_glob.get().await / 4095) as u16;

                let out = ((quantizer.get_quantized_voltage(att_reg)) * 410.0) as u16;
                // let out = att_reg;

                output.set_value(out);
                leds.set(0, Led::Top, led_color.into(), (register_scalled / 16) as u8);
                leds.set(1, Led::Top, led_color.into(), (att_reg / 16) as u8);
                // info!("{}", register_scalled);
                if midi_mode == 1 {
                    let note = (out as u32 * 120 / 4095 + base_note as u32) as u8;
                    midi.send_note_on(note, 4095).await;

                    midi_note.set(note).await;
                }
                if midi_mode == 2 {
                    midi.send_cc(midi_cc as u8 - 1, att_reg).await;
                }

                leds.set(0, Led::Bottom, ATOV_RED, LED_HIGH);
            }

            if inputval <= 406 && oldinputval > 406 {
                leds.set(0, Led::Bottom, ATOV_RED, 0);

                if midi_mode == 1 {
                    let note = midi_note.get().await;
                    midi.send_note_off(note).await;
                }
                register_glob.set(register).await;
            }
            oldinputval = inputval;
        }
    };

    let fut2 = async {
        // fader handling
        loop {
            let chan = fader.wait_for_any_change().await;
            let val = fader.get_all_values();
            let att = att_glob.get().await;
            let prob = prob_glob.get().await;

            if chan == 1 {
                if is_close(att as u16, val[chan]) {
                    latched_glob.set(true).await;
                }

                if latched_glob.get().await {
                    att_glob.set(val[chan] as u32).await;
                    storage
                        .modify_and_save(|s| s.att_saved = val[chan], None)
                        .await;
                }
            }
            if chan == 0 {
                if is_close(prob, val[chan]) {
                    latched_glob.set(true).await;
                }

                if latched_glob.get().await {
                    prob_glob.set(val[chan]).await;
                }
            }
        }
    };

    let rec_flag = app.make_global(false);
    let length_rec = app.make_global(0);

    let fut3 = async {
        loop {
            let shift = buttons.wait_for_down(0).await;
            // latched_glob.set(false).await;
            let mut length = length_rec.get().await;
            if shift && rec_flag.get().await {
                length += 1;
                length_rec.set(length.min(16)).await;
            }
        }
    };

    let fut4 = async {
        let mut shift_old = false;
        let mut button_old = false;
        loop {
            app.delay_millis(1).await;
            if buttons.is_shift_pressed() {
                if !shift_old {
                    latched_glob.set(false).await;
                    shift_old = true;
                    rec_flag.set(true).await;
                    length_rec.set(0).await;
                }
                leds.set(0, Led::Top, RED, (att_glob.get().await / 16) as u8);
            }
            if !buttons.is_shift_pressed() && shift_old {
                latched_glob.set(false).await;
                shift_old = false;
                rec_flag.set(false).await;
                let length = length_rec.get().await;
                if length > 1 {
                    length_glob.set(length - 1).await;
                    // let note = midi_note.get().await;
                    // midi.send_note_off(note).await;
                    storage
                        .modify_and_save(|s| s.length_saved = length, None)
                        .await;
                }
            }

            if buttons.is_button_pressed(0) {
                //button going down
                if !button_old {
                    latched_glob.set(false).await;
                    button_old = true;
                }
            }
            if !buttons.is_button_pressed(0) && button_old {
                latched_glob.set(false).await;
                button_old = false;
                leds.set(0, Led::Bottom, ATOV_RED, 0);
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (att, length, register) = storage
                        .query(|s| (s.att_saved, s.length_saved, s.register_saved))
                        .await;

                    att_glob.set(att as u32).await;
                    length_glob.set(length).await;
                    register_glob.set(register).await;
                    recall_flag.set(true).await;
                    prob_glob.set(0).await;

                    //Add recall routine
                    latched_glob.set(false).await;
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
