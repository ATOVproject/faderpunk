use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{latch::LatchLayer, utils::split_unsigned_value, Brightness, Color, APP_MAX_PARAMS};
use serde::{Deserialize, Serialize};

use libfp::{ext::FromValue, Config, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 4;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Low;

pub static CONFIG: Config<PARAMS> =
    Config::new("CV/OCT to MIDI", "CV and gate to MIDI note converter")
        .add_param(Param::Bool { name: "Bipolar" })
        .add_param(Param::i32 {
            name: "MIDI Channel",
            min: 1,
            max: 16,
        })
        .add_param(Param::i32 {
            name: "Delay (ms)",
            min: 0,
            max: 10,
        })
        .add_param(Param::Color {
            name: "Color",
            variants: &[
                Color::Yellow,
                Color::Pink,
                Color::Blue,
                Color::Red,
                Color::White,
            ],
        });

pub struct Params {
    bipolar: bool,
    midi_channel: i32,

    delay: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bipolar: false,
            midi_channel: 1,

            delay: 0,
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
            bipolar: bool::from_value(values[0]),
            midi_channel: i32::from_value(values[1]),

            delay: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();

        vec.push(self.delay.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; 2],
    muted: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2047, 0],
            muted: false,
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
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let (bipolar, midi_channel, delay, led_color) =
        params.query(|p| (p.bipolar, p.midi_channel, p.delay, p.color));

    let midi = app.use_midi_output(midi_channel as u8 - 1);

    let muted_glob = app.make_global(false);

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    muted_glob.set(storage.query(|s| s.muted));

    if storage.query(|s| s.muted) {
        leds.unset(1, Led::Button);
    } else {
        leds.set(1, Led::Button, led_color, BUTTON_BRIGHTNESS);
        leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
    }

    let input = if bipolar {
        app.make_in_jack(0, Range::_Neg5_5V).await
    } else {
        app.make_in_jack(0, Range::_0_10V).await
    };

    let gate_in = app.make_in_jack(1, Range::_0_10V).await;

    let fut1 = async {
        let mut old_gatein = 0;
        let mut note = 0;
        let mut note_on = false;

        loop {
            app.delay_millis(1).await;

            let gatein = gate_in.get_value();

            if gatein >= 406 && old_gatein < 406 {
                // catching rising edge
                if !muted_glob.get() {
                    app.delay_millis(delay as u64).await;
                    note = ((input.get_value()).min(4095) as i32 + 5) * 120 / 4095;
                    note = (note
                        + (storage.query(|s| s.fader_saved[1]) as i32 * 10 / 4095 - 5) * 12
                        + (storage.query(|s| s.fader_saved[0]) as i32 * 12 / 4095))
                        .clamp(0, 120);
                    midi.send_note_on(note as u8, 4095).await;
                    note_on = true;
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                }
                let note_led = split_unsigned_value((note as u32 * 4095 / 120) as u16);
                leds.set(0, Led::Top, led_color, Brightness::Custom(note_led[0] * 2));
                leds.set(
                    0,
                    Led::Bottom,
                    led_color,
                    Brightness::Custom(note_led[1] * 2),
                );
                leds.set(1, Led::Top, led_color, Brightness::Lower);

                // info!("note on")
            }

            if gatein <= 406 && old_gatein > 406 {
                // catching falling edge
                if note_on {
                    midi.send_note_off(note as u8).await;
                    note_on = false;

                    if muted_glob.get() {
                        leds.unset(1, Led::Button);
                    } else {
                        leds.set(1, Led::Button, led_color, Brightness::Low);
                    }
                }
                leds.unset(1, Led::Top);
            }

            old_gatein = gatein;
        }
    };

    let fut2 = async {
        loop {
            let chan = buttons.wait_for_any_down().await;
            if chan.0 == 0 {
                storage
                    .modify_and_save(
                        |s| {
                            s.fader_saved[0] = 0;
                        },
                        None,
                    )
                    .await;
            }
            if chan.0 == 1 {
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
                    leds.unset(1, Led::Button);
                } else {
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                }
            }
        }
    };
    let fut3 = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => 0,
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {}
                        _ => unreachable!(),
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => 0,
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            storage
                                .modify_and_save(
                                    |s| {
                                        s.fader_saved[chan] = new_value;
                                    },
                                    None,
                                )
                                .await;
                        }
                        LatchLayer::Alt => {}
                        _ => unreachable!(),
                    }
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    if storage.query(|s| s.muted) {
                        leds.unset(1, Led::Button);
                    } else {
                        leds.set(1, Led::Button, led_color, Brightness::Low);
                    }

                    muted_glob.set(storage.query(|s| s.muted));
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
