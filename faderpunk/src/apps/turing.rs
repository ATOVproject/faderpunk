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
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config, Curve,
    MidiCc, MidiChannel, MidiMode, MidiNote, MidiOut, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 8;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Turing",
    "Turing machine, synched to internal clock",
    Color::Blue,
    AppIcon::SequenceSquare,
)
.add_param(Param::MidiMode)
.add_param(Param::MidiChannel {
    name: "MIDI channel",
})
.add_param(Param::MidiCc { name: "CC number" })
.add_param(Param::MidiNote { name: "Base Note" })
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
})
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
})
.add_param(Param::MidiOut);

pub struct Params {
    midi_mode: MidiMode,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_out: MidiOut,
    midi_note: MidiNote,
    gatel: i32,
    color: Color,
    range: Range,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_mode: MidiMode::default(),
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(40),
            midi_note: MidiNote::from(36),
            midi_out: MidiOut::default(),
            gatel: 50,
            color: Color::Blue,
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
            midi_note: MidiNote::from_value(values[3]),
            gatel: i32::from_value(values[4]),
            color: Color::from_value(values[5]),
            range: Range::from_value(values[6]),
            midi_out: MidiOut::from_value(values[7]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    att_saved: u16,
    length_saved: u16,
    register_saved: u16,
    res_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            att_saved: 3000,
            length_saved: 8,
            register_saved: 0,
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
    let (midi_out, midi_mode, midi_cc, led_color, midi_chan, base_note, gatel, range) = params
        .query(|p| {
            (
                p.midi_out,
                p.midi_mode,
                p.midi_cc,
                p.color,
                p.midi_channel,
                p.midi_note,
                p.gatel,
                p.range,
            )
        });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let mut clock = app.use_clock();
    let die = app.use_die();
    let quantizer = app.use_quantizer(range);

    let midi = app.use_midi_output(midi_out, midi_chan);

    let prob_glob = app.make_global(0);

    let recall_flag = app.make_global(false);
    let div_glob = app.make_global(4);
    let midi_note = app.make_global(MidiNote::default());
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    let length_glob = app.make_global(8);

    let resolution = [24, 16, 12, 8, 6, 4, 3, 2];

    leds.set(0, Led::Button, led_color, Brightness::Mid);

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let curve = Curve::Exponential;

    let (mut register, res, mut length) =
        storage.query(|s| (s.register_saved, s.res_saved, s.length_saved));

    div_glob.set(resolution[res as usize / 512]);

    let fut1 = async {
        let mut clkn: usize = 0;
        let mut att_reg: u16;
        loop {
            let div = div_glob.get();
            length = length_glob.get();

            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    if midi_mode == MidiMode::Note {
                        midi.send_note_off(midi_note.get()).await;
                    }
                    register = storage.query(|s| s.register_saved);
                }
                ClockEvent::Tick => {
                    if clkn.is_multiple_of(div) {
                        if (clkn / div).is_multiple_of(length as usize) {
                            let reg_old = storage.query(|s| s.register_saved);
                            if recall_flag.get() {
                                register = reg_old;
                                recall_flag.set(false);

                                let res = storage.query(|s| s.res_saved);
                                length = storage.query(|s| s.length_saved);
                                length_glob.set(length);
                                div_glob.set(resolution[res as usize / 512]);
                                midi.send_note_off(midi_note.get()).await;
                            }

                            if register != reg_old {
                                storage.modify_and_save(|s| s.register_saved = register);
                            }
                        }
                        let prob = prob_glob.get();
                        let rand = die.roll().clamp(100, 3900);

                        let rotation = rotate_select_bit(register, prob, rand, length);
                        register = rotation.0;

                        let register_scalled = scale_to_12bit(register, length as u8);
                        att_reg = ((register_scalled as u32
                            * curve.at(storage.query(|s| s.att_saved)) as u32)
                            / 4095) as u16;

                        let out = quantizer.get_quantized_note(att_reg).await;

                        jack.set_value(out.as_counts(Range::_0_10V));
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((register_scalled / 16) as u8),
                        );
                        match midi_mode {
                            MidiMode::Note => {
                                let note = midi_note.set(out.as_midi() + base_note);
                                midi.send_note_on(note, 4095).await;
                            }
                            MidiMode::Cc => {
                                midi.send_cc(midi_cc, att_reg).await;
                            }
                        }

                        if buttons.is_button_pressed(0) && !buttons.is_shift_pressed() {
                            leds.set(0, Led::Bottom, Color::Red, Brightness::High);
                        }
                    }
                    if clkn % div == (div * gatel as usize / 100).clamp(1, div - 1) {
                        leds.unset(0, Led::Bottom);

                        if midi_mode == MidiMode::Note {
                            midi.send_note_off(midi_note.get()).await;
                        }
                    }

                    clkn += 1;
                }
                ClockEvent::Stop => {
                    if midi_mode == MidiMode::Note {
                        midi.send_note_off(midi_note.get()).await;
                    }
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => prob_glob.get(),
                LatchLayer::Alt => storage.query(|s| s.att_saved),
                LatchLayer::Third => storage.query(|s| s.res_saved),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        prob_glob.set(new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.att_saved = new_value);
                    }
                    LatchLayer::Third => {
                        div_glob.set(resolution[new_value as usize / 512]);
                        if midi_mode == MidiMode::Note {
                            let note = midi_note.get();
                            midi.send_note_off(note).await;
                        }
                        storage.modify_and_save(|s| s.res_saved = new_value);
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
                    shift_old = true;
                    rec_flag.set(true);
                    length_rec.set(0);
                }
                leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((storage.query(|s| s.att_saved) / 16) as u8),
                );
            }
            if !buttons.is_shift_pressed() && shift_old {
                shift_old = false;
                rec_flag.set(false);
                let length = length_rec.get();
                if length >= 1 {
                    length_glob.set(length);
                    storage.modify_and_save(|s| s.length_saved = length);
                }
            }

            if buttons.is_button_pressed(0) && !button_old {
                button_old = true;
            }
            if !buttons.is_button_pressed(0) && button_old {
                button_old = false;
                leds.unset(0, Led::Bottom);
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
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
