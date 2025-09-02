use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate_bipolar, clickless, split_unsigned_value},
    Brightness, Color, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 6;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
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
    .add_param(Param::Bool {
        name: "Mute on release",
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

pub struct Params {
    curve: Curve,
    bipolar: bool,
    midi_channel: i32,
    midi_cc: i32,
    on_release: bool,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            curve: Curve::Linear,
            bipolar: false,
            midi_channel: 1,
            midi_cc: 32,
            on_release: false,
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
            curve: Curve::from_value(values[0]),
            bipolar: bool::from_value(values[1]),
            midi_channel: i32::from_value(values[2]),
            midi_cc: i32::from_value(values[3]),
            on_release: bool::from_value(values[4]),
            color: Color::from_value(values[5]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.on_release.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

// TODO: Make a macro to generate this.
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
    let (curve, midi_chan, midi_cc, on_release, bipolar, led_color) = params.query(|p| {
        (
            p.curve,
            p.midi_channel,
            p.midi_cc,
            p.on_release,
            p.bipolar,
            p.color.into(),
        )
    });

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_chan as u8);

    let muted_glob = app.make_global(storage.query(|s| s.muted));
    let output_glob = app.make_global(0);
    let latch_layer_glob = app.make_global(LatchLayer::Main);

    if muted_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Lower);
    }

    let jack = if !bipolar {
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    let main_loop = async {
        let mut latch = app.make_latch(fader.get_value());
        let mut main_layer_value = fader.get_value();

        loop {
            app.delay_millis(1).await;

            let latch_active_layer =
                latch_layer_glob.set(LatchLayer::from(buttons.is_shift_pressed()));
            let att_layer_value = storage.query(|s| s.att_saved);

            let latch_target_value = match latch_active_layer {
                LatchLayer::Main => main_layer_value,
                LatchLayer::Alt => att_layer_value,
                _ => unreachable!(),
            };

            if let Some(new_value) =
                latch.update(fader.get_value(), latch_active_layer, latch_target_value)
            {
                match latch_active_layer {
                    LatchLayer::Main => {
                        main_layer_value = new_value;
                    }
                    LatchLayer::Alt => {
                        // Update storage but don't save yet
                        storage.modify(|s| s.att_saved = new_value);
                    }
                    _ => unreachable!(),
                }
            }

            // Calculate output values
            let muted = muted_glob.get();

            let val = if muted {
                if bipolar {
                    2047
                } else {
                    0
                }
            } else {
                curve.at(main_layer_value)
            };

            let out = output_glob.modify(|o| clickless(*o, val));
            let att_layer_value = storage.query(|s| s.att_saved);
            let attenuated = if bipolar {
                attenuate_bipolar(out, att_layer_value)
            } else {
                ((out as u32 * att_layer_value as u32) / 4095) as u16
            };

            jack.set_value(attenuated);

            // Update LEDs
            match latch_active_layer {
                LatchLayer::Main => {
                    if bipolar {
                        let led1 = split_unsigned_value(out);
                        leds.set(0, Led::Top, led_color, Brightness::Custom(led1[0]));
                        leds.set(0, Led::Bottom, led_color, Brightness::Custom(led1[1]));
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            led_color,
                            Brightness::Custom((attenuated as f32 / 16.) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
                }
                LatchLayer::Alt => {
                    if bipolar {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                        leds.set(
                            0,
                            Led::Bottom,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                    } else {
                        leds.set(
                            0,
                            Led::Top,
                            Color::Red,
                            Brightness::Custom((att_layer_value / 16) as u8),
                        );
                        leds.unset(0, Led::Bottom);
                    }
                }
                _ => unreachable!(),
            }
        }
    };

    let button_handler = async {
        loop {
            if on_release {
                buttons.wait_for_up(0).await;
            } else {
                buttons.wait_for_down(0).await;
            }
            let muted = storage
                .modify_and_save(
                    |s| {
                        s.muted = !s.muted;
                        s.muted
                    },
                    None,
                )
                .await;
            muted_glob.set(muted);
            if muted {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, led_color, Brightness::Lower);
            }
        }
    };

    let fader_event_handler = async {
        loop {
            fader.wait_for_any_change().await;

            match latch_layer_glob.get() {
                LatchLayer::Main => {
                    // Send MIDI
                    midi.send_cc(midi_cc as u8, output_glob.get()).await;
                }
                LatchLayer::Alt => {
                    // Now we commit to storage
                    storage.save(None).await;
                }
                _ => unreachable!(),
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let muted = storage.query(|s| s.muted);
                    muted_glob.set(muted);
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Lower);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(
        main_loop,
        button_handler,
        fader_event_handler,
        scene_handler,
    )
    .await;
}
