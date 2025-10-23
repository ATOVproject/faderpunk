use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;

use midly::MidiMessage;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{bits_7_16, clickless, scale_bits_7_12},
    AppIcon, Brightness, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 7;

const LED_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new(
    "MIDI to CV",
    "Multifunctional MIDI to CV",
    Color::Cyan,
    AppIcon::KnobRound,
)
.add_param(Param::Enum {
    name: "Mode",
    variants: &["CC", "Pitch", "Gate", "Velocity", "AT", "Bend", "Note Gate"],
})
.add_param(Param::Curve {
    name: "Curve",
    variants: &[Curve::Linear, Curve::Logarithmic, Curve::Exponential],
})
.add_param(Param::i32 {
    name: "MIDI Channel",
    min: 1,
    max: 16,
})
.add_param(Param::i32 {
    name: "MIDI CC",
    min: 1,
    max: 128,
})
.add_param(Param::i32 {
    name: "Bend Range",
    min: 1,
    max: 24,
})
.add_param(Param::i32 {
    name: "note",
    min: 1,
    max: 128,
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
    mode: usize,
    curve: Curve,
    midi_channel: i32,
    midi_cc: i32,
    bend_range: i32,
    note: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            mode: 0,
            curve: Curve::Linear,
            midi_channel: 1,
            midi_cc: 32,
            bend_range: 12,
            note: 36,
            color: Color::Cyan,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            mode: usize::from_value(values[0]),
            curve: Curve::from_value(values[1]),
            midi_channel: i32::from_value(values[2]),
            midi_cc: i32::from_value(values[3]),
            bend_range: i32::from_value(values[4]),
            note: i32::from_value(values[5]),
            color: Color::from_value(values[6]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.mode.into()).unwrap();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.bend_range.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
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
    let (midi_chan, midi_cc, curve, bend_range, led_color, note, mode) = params.query(|p| {
        (
            p.midi_channel,
            p.midi_cc,
            p.curve,
            p.bend_range,
            p.color,
            p.note,
            p.mode,
        )
    });

    let mut midi_in = app.use_midi_input(midi_chan as u8 - 1);
    let muted_glob = app.make_global(false);

    let offset_glob = app.make_global(0);
    let pitch_glob = app.make_global(0);
    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let muted = storage.query(|s| (s.muted));
    muted_glob.set(muted);

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let jack = if mode != 5 {
        // info!("range 0-10V");
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        // info!("range +/-5V");
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    if mode == 5 || mode == 1 {
        jack.set_value(2048);
        offset_glob.set(2048);
    } else {
        jack.set_value(0);
    }

    // let jack = app.make_out_jack(0, Range::_0_10V).await;
    // jack.set_value(0);

    let fut1 = async {
        let mut outval = 0;
        let mut val = fader.get_value();
        let mut fadval = fader.get_value();
        let mut attval = 0;

        loop {
            app.delay_millis(1).await;
            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            if mode == 0 || mode == 4 {
                let muted = muted_glob.get();
                if !buttons.is_shift_pressed() {
                    fadval = fader.get_value();
                }
                let att = storage.query(|s| (s.att_saved));
                // info!("{}", att);
                let offset = offset_glob.get();

                // if buttons.is_shift_pressed() {

                if muted {
                    val = 0;
                } else {
                    val = curve.at(fadval + offset);
                }

                outval = clickless(outval, val);
                attval = ((outval as u32 * att as u32) / 4095) as u16;

                jack.set_value(attval);
                if latch_active_layer == LatchLayer::Alt {
                    leds.set(
                        0,
                        Led::Top,
                        Color::Red,
                        Brightness::Custom((att / 16) as u8),
                    );
                    leds.unset(0, Led::Bottom);
                } else {
                    leds.set(
                        0,
                        Led::Top,
                        led_color,
                        Brightness::Custom((attval as f32 / 16.0) as u8),
                    );
                }
            }
            if mode == 5 {
                if !muted_glob.get() {
                    let offset = offset_glob.get();
                    outval = clickless(outval, offset);
                    jack.set_value(outval);
                } else {
                    let offset = 2048;
                    outval = clickless(outval, offset);
                    jack.set_value(outval);
                }
            }
            if mode == 1 {
                let offset = if !muted_glob.get() {
                    offset_glob.get()
                } else {
                    2047
                };

                let pitch = pitch_glob.get();
                outval = clickless(outval, offset);
                let out = (pitch as i32 + outval as i32 - 2047).clamp(0, 4095) as u16;
                // //info!("outval ={}, pitch = {} out = {}", outval, pitch, out);
                jack.set_value(out);
            }

            // match latch_active_layer {
            //     LatchLayer::Main => {}
            //     LatchLayer::Alt => {
            //         leds.set(
            //             0,
            //             Led::Top,
            //             Color::Red,
            //             Brightness::Custom((att / 16) as u8),
            //         );
            //         leds.set(0, Led::Bottom, Color::Red, Brightness::Custom(0));
            //     }
            //     _ => unreachable!(),
            // }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;

            let muted = storage.modify_and_save(|s| {
                s.muted = !s.muted;
                s.muted
            });
            muted_glob.set(muted);
            if muted {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
            }
            if mode == 3 {
                jack.set_value(0);
                leds.unset(0, Led::Top);
            }
        }
    };
    let fut3 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Alt => storage.query(|s| s.att_saved),
                LatchLayer::Main => 0,

                _ => unreachable!(),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {}
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.att_saved = new_value);
                    }
                    _ => unreachable!(),
                }
            }

            // let fader_val = fader.get_value();

            // if buttons.is_shift_pressed() && latched_glob.get() {
            //     att_glob.set(fader_val);
            //     storage
            //         .modify_and_save(
            //             |s| {
            //                 s.att_saved = fader_val;
            //                 s.att_saved
            //             },
            //             None,
            //         )
            //         .await;
            // }
        }
    };

    let fut4 = async {
        let mut note_num = 0;
        loop {
            match midi_in.wait_for_message().await {
                MidiMessage::Controller { controller, value } => {
                    if mode == 0 {
                        if bits_7_16(controller) == midi_cc as u16 {
                            let val = scale_bits_7_12(value);
                            offset_glob.set(val);
                        }
                    }
                }
                MidiMessage::NoteOn { key, vel } => {
                    if mode == 1 {
                        if !muted_glob.get() {
                            let mut note_in = bits_7_16(key);
                            note_in = (note_in as u32 * 410 / 12) as u16;
                            let oct = (fader.get_value() as i32 * 10 / 4095) - 5;
                            let note_out = (note_in as i32 + oct * 410).clamp(0, 4095) as u16;
                            // jack.set_value(note_out);
                            pitch_glob.set(note_out);
                            leds.set(
                                0,
                                Led::Top,
                                led_color,
                                Brightness::Custom((note_out / 16) as u8),
                            );
                        }
                    }
                    if mode == 2 {
                        if !muted_glob.get() {
                            jack.set_value(4095);
                            note_num += 1;
                            leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                        } else {
                            note_num = 0;
                        }

                        //info!("note on num = {}", note_num);
                    }
                    if mode == 6 && bits_7_16(key) == note as u16 {
                        if !muted_glob.get() {
                            jack.set_value(4095);
                            note_num += 1;
                            leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                        } else {
                            note_num = 0;
                        }
                    }
                    if mode == 3 {
                        let vel_out = if !muted_glob.get() {
                            scale_bits_7_12(vel)
                        } else {
                            0
                        };
                        jack.set_value(vel_out);

                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((vel_out / 16) as u8),
                        );
                        //info!("Velocity: {} ", vel_out)
                    }
                }
                MidiMessage::NoteOff { key, vel } => {
                    if mode == 2 {
                        note_num = (note_num - 1).max(0);
                        //info!("note off num = {}", note_num);
                        if note_num == 0 {
                            jack.set_value(0);
                            leds.unset(0, Led::Top);
                        }
                    }
                    if mode == 6 && bits_7_16(key) == note as u16 {
                        note_num = (note_num - 1).max(0);
                        //info!("note off num = {}", note_num);
                        if note_num == 0 {
                            jack.set_value(0);
                            leds.unset(0, Led::Top);
                        }
                    }
                }
                MidiMessage::PitchBend { bend } => {
                    //info!("mode = {}", mode);
                    if mode == 5 || mode == 1 {
                        let out = (bend.as_f32() * bend_range as f32 * 410. / 12. + 2048.) as u16;
                        offset_glob.set(out);
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((bend.as_f32() * 255.0) as u8),
                        );
                        leds.set(
                            0,
                            Led::Bottom,
                            led_color,
                            Brightness::Custom((bend.as_f32() * -255.0) as u8),
                        );
                        //info!("Bend! = {}, bend range = {}", bend.as_f32(), out);
                    }
                    // if mode == 1 {
                    //     let out = (bend.as_f32() * bend_range as f32 * 410. / 12. + 2048.) as u16;
                    //     offset_glob.set(out);
                    //     //info!("Bend! = {}, bend range = {}", bend.as_f32(), out);
                    // }
                }
                MidiMessage::ChannelAftertouch { vel } => {
                    if mode == 4 {
                        let val = scale_bits_7_12(vel);
                        offset_glob.set(val);
                    }
                }

                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load_from_scene(scene).await;
                    let muted = storage.query(|s| s.muted);
                    muted_glob.set(muted);
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, scene_handler).await;
}
