// Todo :
// Save div, mute, attenuation - Added the saving slots, need to add write/read in the app.
// Add attenuator (shift + fader)

use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent, RGB8,
};

use libfp::{
    ext::FromValue,
    latch::LatchLayer,
    utils::{attenuate, attenuate_bipolar, split_unsigned_value},
    Brightness, Color, Config, Curve, Param, Range, Value, APP_MAX_PARAMS,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

const LED_COLOR: Color = Color::Violet;

pub static CONFIG: Config<PARAMS> = Config::new("Random CC/CV", "Generate random values on clock")
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "MIDI CC",
        min: 1,
        max: 128,
    });

pub struct Params {
    midi_channel: i32,
    midi_cc: i32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: 1,
            midi_cc: 32,
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
            midi_cc: i32::from_value(values[1]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_save: bool,
    att_saved: u16,
    slew_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_save: false,
            att_saved: 4096,
            slew_saved: 0,
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
    let (midi_chan, cc) = params.query(|p| (p.midi_channel, p.midi_cc));

    let mut clock = app.use_clock();
    let rnd = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let midi = app.use_midi_output(midi_chan as u8 - 1);
    let output = app.make_out_jack(0, Range::_Neg5_5V).await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let val_glob = app.make_global(0);
    let new_color = app.make_global(false);

    let latched_glob = app.make_global(false);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;
    let mut val = 2048;

    let curve = Curve::Logarithmic;

    let (res, mute) = storage.query(|s| (s.fader_saved, s.mute_save));

    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        output.set_value(2047);
        midi.send_cc(cc as u8, 0).await;
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
    }

    let fut1 = async {
        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();

                    let div = div_glob.get();
                    if clkn % div == 0 && !muted {
                        val_glob.set(rnd.roll());
                        new_color.set(true);
                    }
                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_any_down().await;
            if buttons.is_shift_pressed() {
                let muted = glob_muted.toggle();

                storage
                    .modify_and_save(
                        |s| {
                            s.mute_save = muted;
                            s.mute_save
                        },
                        None,
                    )
                    .await;

                if muted {
                    output.set_value(2047);
                    midi.send_cc(cc as u8, 0).await;
                    leds.unset_all();
                } else {
                    leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
                }
            }
        }
    };

    let fut3 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change_at(0).await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.fader_saved),
                LatchLayer::Alt => storage.query(|s| s.att_saved),
                LatchLayer::Third => storage.query(|s| s.slew_saved),
                _ => unreachable!(),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        div_glob.set(resolution[new_value as usize / 345]);
                        storage
                            .modify_and_save(|s| s.fader_saved = new_value, None)
                            .await;
                    }
                    LatchLayer::Alt => {
                        storage
                            .modify_and_save(|s| s.att_saved = new_value, None)
                            .await;
                    }
                    LatchLayer::Third => {
                        storage
                            .modify_and_save(|s| s.slew_saved = new_value, None)
                            .await
                    }
                    _ => unreachable!(),
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (res, mute, att) =
                        storage.query(|s| (s.fader_saved, s.mute_save, s.att_saved));

                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.set(0, Led::Button, LED_COLOR, Brightness::Lower);
                        output.set_value(2047);
                        midi.send_cc(cc as u8, 0).await;
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    }
                    latched_glob.set(false);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    let shift = async {
        let mut out = 0.;
        let mut r = (rnd.roll() / 16) as u8;
        let mut g = (rnd.roll() / 16) as u8;
        let mut b = (rnd.roll() / 16) as u8;
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

            let att = storage.query(|s| (s.att_saved));

            let midival = attenuate(val_glob.get(), att);
            let jackval = attenuate_bipolar(val_glob.get(), att);

            midi.send_cc(cc as u8, midival).await;

            out = slew_2(out, jackval, curve.at(storage.query(|s| s.slew_saved)));
            // out = slew_2(out, jackval, 4095);
            output.set_value(out as u16);
            if new_color.get() {
                new_color.set(false);
                r = (rnd.roll() / 16) as u8;
                g = (rnd.roll() / 16) as u8;
                b = (rnd.roll() / 16) as u8;
            }

            if latch_active_layer == LatchLayer::Main {
                let ledj = split_unsigned_value(out as u16);
                let color = Color::Custom(r, g, b);
                leds.set(0, Led::Top, color, Brightness::Custom(ledj[0]));
                leds.set(0, Led::Bottom, color, Brightness::Custom(ledj[1]));
                leds.set(0, Led::Button, color, Brightness::Lower);
            }
            if latch_active_layer == LatchLayer::Alt {
                leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((att / 16) as u8),
                );
                leds.set(0, Led::Button, Color::Red, Brightness::Low);
            }
            if latch_active_layer == LatchLayer::Third {
                leds.set(
                    0,
                    Led::Top,
                    Color::Green,
                    Brightness::Custom((storage.query(|s| s.slew_saved) / 16) as u8),
                );
                leds.set(0, Led::Button, Color::Green, Brightness::Low);
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}

fn slew_2(prev: f32, input: u16, slew: u16) -> f32 {
    (prev * slew as f32 + input as f32) / (slew + 1) as f32
}
