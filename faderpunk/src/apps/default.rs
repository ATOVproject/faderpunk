use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    colors::RED,
    utils::{attenuate_bipolar, clickless, is_close, split_unsigned_value},
    Brightness, Color,
};
use serde::{Deserialize, Serialize};

use libfp::{Config, Curve, Param, Range, Value};

use crate::app::{App, AppStorage, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent};

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
            Color::Purple,
            Color::Teal,
            Color::Red,
            Color::White,
        ],
    });

const LED_BRIGHTNESS: Brightness = Brightness::Lower;

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

// TODO: Make a macro to generate this.
pub struct Params<'a> {
    curve: ParamSlot<'a, Curve, PARAMS>,
    bipolar: ParamSlot<'a, bool, PARAMS>,
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    midi_cc: ParamSlot<'a, i32, PARAMS>,
    on_release: ParamSlot<'a, bool, PARAMS>,
    color: ParamSlot<'a, Color, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [
            Value::Curve(Curve::Linear),
            Value::bool(false),
            Value::i32(1),
            Value::i32(32),
            Value::bool(false),
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
        on_release: ParamSlot::new(&param_store, 4),
        color: ParamSlot::new(&param_store, 5),
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
    let midi_chan = params.midi_channel.get().await;
    let midi_cc = params.midi_cc.get().await;
    let curve = params.curve.get().await;
    let led_color = params.color.get().await;
    let bipolar = params.bipolar.get().await;
    let on_release = params.on_release.get().await;

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_chan as u8);

    let muted_glob = app.make_global(false);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global(false);

    let muted = storage.query(|s| s.muted).await;
    let att = storage.query(|s| s.att_saved).await;
    muted_glob.set(muted);
    att_glob.set(att);

    if muted {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
    }

    let jack = if !bipolar {
        app.make_out_jack(0, Range::_0_10V).await
    } else {
        app.make_out_jack(0, Range::_Neg5_5V).await
    };

    let fut1 = async {
        let mut outval = 0.;
        let mut val = fader.get_value();
        let mut fadval = fader.get_value();
        let mut old_midi = 0;
        let mut attval = 0;
        let mut shift_old = false;

        loop {
            app.delay_millis(1).await;
            let muted = muted_glob.get();
            if !buttons.is_shift_pressed() {
                fadval = fader.get_value();
            }
            let att = att_glob.get();

            // if buttons.is_shift_pressed() {
            if bipolar {
                if muted {
                    val = 2047;
                } else {
                    val = curve.at(fadval);
                }
                if !buttons.is_shift_pressed() {
                    let led1 = split_unsigned_value(outval as u16);
                    leds.set(0, Led::Top, led_color.into(), Brightness::Custom(led1[0]));
                    leds.set(
                        0,
                        Led::Bottom,
                        led_color.into(),
                        Brightness::Custom(led1[1]),
                    );
                } else {
                    leds.set(0, Led::Top, RED, Brightness::Custom((att / 16) as u8));
                    leds.set(0, Led::Bottom, RED, Brightness::Custom((att / 16) as u8));
                }
                outval = clickless(outval, val);
                attval = attenuate_bipolar(outval as u16, att);
            } else {
                if muted {
                    val = 0;
                } else {
                    val = curve.at(fadval);
                }
                if buttons.is_shift_pressed() {
                    leds.set(0, Led::Top, RED, Brightness::Custom((att / 16) as u8));
                    leds.unset(0, Led::Bottom);
                } else {
                    leds.set(
                        0,
                        Led::Top,
                        led_color.into(),
                        Brightness::Custom((outval / 16.) as u8),
                    );
                }
                outval = clickless(outval, val);
                attval = ((outval as u32 * att as u32) / 4095) as u16;
            }

            jack.set_value(attval);

            if old_midi != outval as u16 / 32 {
                midi.send_cc(midi_cc as u8, outval as u16).await;
                old_midi = outval as u16 / 32;
            }

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false);

                shift_old = false;
            }
        }
    };

    // Handles toggle mute logic
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
                leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change().await;
            let fader_val = fader.get_value();

            if !latched_glob.get() && is_close(fader_val, att_glob.get()) {
                latched_glob.set(true);
            }
            if buttons.is_shift_pressed() && latched_glob.get() {
                att_glob.set(fader_val);
                storage
                    .modify_and_save(
                        |s| {
                            s.att_saved = fader_val;
                            s.att_saved
                        },
                        None,
                    )
                    .await;
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let muted = storage.query(|s| s.muted).await;
                    muted_glob.set(muted);
                    if muted {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
                    }
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, button_handler, fut3, scene_handler).await;
}
