//Bug :
// No midi out when recording

use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    colors::RED,
    utils::{attenuate, attenuate_bipolar, clickless, split_unsigned_value},
    Brightness, Color,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::{
    app::{App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot},
    storage::ParamStore,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

pub static CONFIG: Config<PARAMS> = Config::new("Automator", "Fader movement recording")
    .add_param(Param::Curve {
        name: "Curve",
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    .add_param(Param::Bool { name: "Bipolar" })
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
    .add_param(Param::Color {
        name: "Color",
        variants: &[
            Color::Yellow,
            Color::Purple,
            Color::Teal,
            Color::Red,
            Color::White,
        ],
    });

#[derive(Serialize, Deserialize)]
pub struct Storage {
    att_saved: u16,
    // buffer_saved: Arr<u16, 384>,
    // length_saved: usize,
}

impl AppStorage for Storage {}

pub struct Params<'a> {
    curve: ParamSlot<'a, Curve, PARAMS>,
    bipolar: ParamSlot<'a, bool, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    midi_cc: ParamSlot<'a, i32, PARAMS>,
    color: ParamSlot<'a, Color, PARAMS>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            att_saved: 4095,
            // buffer_saved: Arr::new([0; 384]),
            // length_saved: 384,
        }
    }
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [
            Value::Curve(Curve::Linear),
            Value::bool(false),
            Value::i32(1),
            Value::i32(32),
            Value::Color(Color::Yellow),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        curve: ParamSlot::new(&param_store, 0),
        bipolar: ParamSlot::new(&param_store, 1),
        midi_channel: ParamSlot::new(&param_store, 2),
        midi_cc: ParamSlot::new(&param_store, 3),
        color: ParamSlot::new(&param_store, 4),
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
    let led_color = params.color.get().await;

    let midi_chan = params.midi_channel.get().await;
    let cc: u8 = params.midi_cc.get().await as u8;
    let midi = app.use_midi_output(midi_chan as u8 - 1);
    let curve = params.curve.get().await;

    let mut clock = app.use_clock();

    let rec_flag = app.make_global(false);
    let offset_glob = app.make_global(0);
    let buffer_glob = app.make_global([0; 384]);
    let recording_glob = app.make_global(false);
    let length_glob = app.make_global(384);
    let index_glob = app.make_global(0);
    let latched = app.make_global(false);

    let jack = if !params.bipolar.get().await {
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    let mut last_midi = 0;

    let mut index = 0;
    let mut recording = false;
    let mut buffer = [0; 384];
    let mut length = 384;
    let att_glob = app.make_global(4095);

    // let (buffer_saved, length_saved) = storage
    //     .query(|s| (s.buffer_saved.get(), s.length_saved))
    //     .await;
    // buffer_glob.set(buffer_saved).await;
    // length_glob.set(length_saved).await;

    att_glob.set(storage.query(|s| s.att_saved).await);

    leds.set(0, Led::Button, led_color.into(), Brightness::Lower);

    let update_output = async {
        let mut outval = 0.;
        let mut shift_old = false;
        loop {
            app.delay_millis(1).await;
            let index = index_glob.get();
            let buffer = buffer_glob.get();
            let mut offset = offset_glob.get();
            let att = att_glob.get();
            let color = if recording_glob.get() {
                RED
            } else {
                led_color.into()
            };

            if latched.get() {
                offset = fader.get_value();
                offset_glob.set(offset);
            }

            let val = (buffer[index] + offset).min(4095);

            outval = clickless(outval, val);
            if !params.bipolar.get().await {
                let out = curve.at(attenuate(outval as u16, att));
                jack.set_value(out);
                if !buttons.is_shift_pressed() {
                    leds.set(0, Led::Top, color, Brightness::Custom((out / 16) as u8));
                } else {
                    leds.set(0, Led::Top, RED, Brightness::Custom((att / 16) as u8));
                }
            } else {
                let out = curve.at(attenuate_bipolar(outval as u16, att));
                jack.set_value(out);
                if !buttons.is_shift_pressed() {
                    let ledint = split_unsigned_value(out);
                    leds.set(0, Led::Top, color, Brightness::Custom(ledint[0]));
                    leds.set(0, Led::Bottom, color, Brightness::Custom(ledint[1]));
                } else {
                    let ledint = split_unsigned_value(att);
                    leds.set(0, Led::Top, RED, Brightness::Custom(ledint[0]));
                    leds.set(0, Led::Bottom, RED, Brightness::Custom(ledint[1]));
                }
            }
            if last_midi / 16 != (val) / 16 {
                midi.send_cc(cc as u8, attenuate(outval as u16, att)).await;
                last_midi = outval as u16;
            };

            if !shift_old && buttons.is_shift_pressed() {
                latched.set(false);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched.set(false);

                shift_old = false;
            }
        }
    };

    let fut1 = async {
        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    index = 0;
                    recording = false;
                    recording_glob.set(recording);
                }
                ClockEvent::Tick => {
                    length = length_glob.get();

                    index %= length;

                    index_glob.set(index);
                    recording = recording_glob.get();

                    if index == 0 && recording {
                        //stop recording at max length
                        recording = false;
                        recording_glob.set(recording);
                        length = 384;
                        length_glob.set(length);
                        buffer_glob.set(buffer);
                        offset_glob.set(0);
                        latched.set(false);
                        // storage
                        //     .modify(|s| {
                        //         s.buffer_saved.set(buffer);
                        //         s.length_saved = length;
                        //     })
                        //     .await;
                    }

                    if rec_flag.get() && index % 96 == 0 {
                        index = 0;
                        recording = true;
                        buffer = [0; 384];
                        buffer_glob.set(buffer);
                        recording_glob.set(recording);
                        rec_flag.set(false);
                        length = 384;
                        length_glob.set(length);
                        latched.set(true);
                    }

                    if recording {
                        let val = fader.get_value();
                        buffer[index] = val;
                        leds.set(0, Led::Button, RED, Brightness::Lower);
                    } else {
                        leds.set(0, Led::Button, led_color.into(), Brightness::Lower);
                    }

                    if recording && !buttons.is_button_pressed(0) && index % 96 == 0 && index != 0 {
                        //finish recording
                        recording = !recording;
                        recording_glob.set(recording);
                        length = index;
                        length_glob.set(length);
                        buffer_glob.set(buffer);
                        offset_glob.set(0);
                        latched.set(false);
                        // storage
                        //     .modify(|s| {
                        //         s.buffer_saved.set(buffer);
                        //         s.length_saved = length;
                        //     })
                        //     .await;
                    }

                    if index == 0 {
                        leds.unset(0, Led::Button);
                    }

                    index += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            fader.wait_for_change().await;
            let val = fader.get_value();
            if !buttons.is_shift_pressed() {
                if is_close(val, offset_glob.get()) && !latched.get() {
                    latched.set(true)
                }
            } else {
                if !latched.get() && is_close(val, att_glob.get()) {
                    latched.set(true);
                }
                if latched.get() {
                    att_glob.set(val);

                    leds.set(0, Led::Top, RED, Brightness::Custom((val / 16) as u8));

                    storage
                        .modify_and_save(
                            |s| {
                                s.att_saved = val;
                                s.att_saved
                            },
                            None,
                        )
                        .await;
                }
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            if buttons.is_shift_pressed() {
                recording_glob.set(false);
                buffer_glob.set([0; 384]);
                length_glob.set(384);
                leds.set(0, Led::Button, led_color.into(), Brightness::Lower);
                latched.set(false);
            } else {
                rec_flag.set(true);
            }
        }
    };

    // let scene_handler = async {
    //     loop {
    //         match app.wait_for_scene_event().await {
    //             SceneEvent::LoadSscene(scene) => {
    //                 storage.load(Some(scene)).await;
    //                 let (buffer_saved, length_saved) = storage
    //                     .query(|s| (s.buffer_saved.get(), s.length_saved))
    //                     .await;
    //                 buffer_glob.set(buffer_saved).await;
    //                 length_glob.set(length_saved).await;
    //                 latched.set(false).await;
    //                 offset_glob.set(0).await;
    //             }
    //             SceneEvent::SaveScene(scene) => {
    //                 storage.save(Some(scene)).await;
    //             }
    //         }
    //     }
    // };

    join4(update_output, fut1, fut2, fut3).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
