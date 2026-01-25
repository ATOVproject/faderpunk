// Todo
// Quantizer

use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, Color, Config, Curve, MidiCc,
    MidiChannel, MidiMode, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 7;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Turing+",
    "Turing machine, with clock input",
    Color::Pink,
    AppIcon::SequenceSquare,
)
.add_param(Param::MidiMode)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "CC number" })
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
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
})
.add_param(Param::MidiNote { name: "Base note" })
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_mode: MidiMode,
    midi_note: MidiNote,
    midi_out: MidiOut,
    color: Color,
    range: Range,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(38),
            midi_mode: MidiMode::default(),
            midi_note: MidiNote::from(36),
            midi_out: MidiOut::default(),
            color: Color::Pink,
            range: Range::_0_5V,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_mode: MidiMode::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            midi_cc: MidiCc::from_value(values[2]),
            color: Color::from_value(values[3]),
            range: Range::from_value(values[4]),
            midi_note: MidiNote::from_value(values[5]),
            midi_out: MidiOut::from_value(values[6]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
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
    let (midi_mode, midi_cc, base_note, midi_out, midi_chan, led_color, range) =
        params.query(|p| {
            (
                p.midi_mode,
                p.midi_cc,
                p.midi_note,
                p.midi_out,
                p.midi_channel,
                p.color,
                p.range,
            )
        });

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let die = app.use_die();
    let midi = app.use_midi_output(midi_out, midi_chan);

    // let mut prob_glob = app.make_global_with_store(0, StorageSlot::A);
    // let mut length_glob = app.make_global_with_store(15, StorageSlot::B);
    // let mut att_glob = app.make_global_with_store(4095, StorageSlot::C);

    let prob_glob = app.make_global(0);
    let length_glob = app.make_global(15_u16);

    let register_glob = app.make_global(0);
    let recall_flag = app.make_global(false);
    let midi_note = app.make_global(MidiNote::from(0));

    let quantizer = app.use_quantizer(range);

    leds.set(0, Led::Button, led_color, Brightness::Mid);
    leds.set(1, Led::Button, led_color, Brightness::Mid);

    let input = app.make_in_jack(0, range).await;
    let output = app.make_out_jack(1, range).await;

    let (length, mut register) = storage.query(|s| (s.length_saved, s.register_saved));

    length_glob.set(length);
    register_glob.set(register);
    let curve = Curve::Linear;

    let fut1 = async {
        let mut att_reg: u16;
        let mut oldinputval = 0;

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
                storage.modify_and_save(|s| {
                    s.register_saved = register;
                });

                let register_scalled = scale_to_12bit(register, length as u8);
                att_reg = (register_scalled as u32
                    * curve.at(storage.query(|s| s.att_saved)) as u32
                    / 4095) as u16;

                let out = quantizer.get_quantized_note(att_reg).await;

                output.set_value(out.as_counts(range));
                leds.set(
                    0,
                    Led::Top,
                    led_color,
                    Brightness::Custom((register_scalled / 16) as u8),
                );
                leds.set(
                    1,
                    Led::Top,
                    led_color,
                    Brightness::Custom((att_reg / 16) as u8),
                );

                if let MidiMode::Note = midi_mode {
                    let note = base_note + out.as_midi();
                    midi.send_note_on(note, 4095).await;
                    midi_note.set(note);
                }

                if let MidiMode::Cc = midi_mode {
                    midi.send_cc(midi_cc, att_reg).await;
                }

                leds.set(0, Led::Bottom, Color::Red, Brightness::High);
            }

            if inputval <= 406 && oldinputval > 406 {
                leds.set(0, Led::Bottom, Color::Red, Brightness::Off);

                if let MidiMode::Note = midi_mode {
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
                    storage.modify_and_save(|s| {
                        s.att_saved = new_value;
                    });
                }
            }
        }
    };

    let rec_flag = app.make_global(false);
    let length_rec = app.make_global(0);

    let fut3 = async {
        loop {
            let shift = buttons.wait_for_down(0).await;
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

            if buttons.is_shift_pressed() && !shift_old {
                shift_old = true;
                rec_flag.set(true);
                length_rec.set(0);
            }
            if !buttons.is_shift_pressed() && shift_old {
                shift_old = false;
                rec_flag.set(false);
                let length = length_rec.get();
                if length > 1 {
                    length_glob.set(length);
                    storage.modify_and_save(|s| s.length_saved = length);
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (length, register) = storage.query(|s| (s.length_saved, s.register_saved));

                    length_glob.set(length);
                    register_glob.set(register);
                    recall_flag.set(true);
                    prob_glob.set(0);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
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
