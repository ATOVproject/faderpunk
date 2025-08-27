use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{Brightness, Color, APP_MAX_PARAMS};
use serde::{Deserialize, Serialize};

use libfp::{ext::FromValue, Config, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 5;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> = Config::new("CV/OCT to MIDI", "")
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
    midi_cc: i32,
    delay: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bipolar: false,
            midi_channel: 1,
            midi_cc: 32,
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
            midi_cc: i32::from_value(values[2]),
            delay: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.delay.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

// TODO: Make a macro to generate this.
#[derive(Serialize, Deserialize)]
pub struct Storage {
    muted: bool,
    att_saved: u16,
    offset_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            muted: false,
            att_saved: 4095,
            offset_saved: 0,
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
    let fader = app.use_faders();
    let leds = app.use_leds();

    let (bipolar, midi_channel, midi_cc, delay, led_color) =
        params.query(|p| (p.bipolar, p.midi_channel, p.midi_cc, p.delay, p.color));

    let midi = app.use_midi_output(midi_channel as u8 - 1);

    let muted_glob = app.make_global(false);
    let att_glob = app.make_global(4095);
    let latched_glob = app.make_global(false);
    let offset_glob = app.make_global(0);

    muted_glob.set(storage.query(|s| s.muted));
    att_glob.set(storage.query(|s| s.att_saved));
    offset_glob.set(storage.query(|s| s.offset_saved));

    if storage.query(|s| s.muted) {
        leds.unset(1, Led::Button);
    } else {
        leds.set(1, Led::Button, led_color, BUTTON_BRIGHTNESS);
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
                    note =
                        (note + (fader.get_value_at(0) as i32 * 10 / 4095 - 5) * 12).clamp(0, 120);
                    midi.send_note_on(note as u8, 4095).await;
                    note_on = true;
                    leds.set(1, Led::Button, led_color, Brightness::Low);
                }
                leds.set(0, Led::Top, led_color, Brightness::Custom((note * 2) as u8));
                leds.set(1, Led::Top, led_color, Brightness::Low);

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
                        leds.set(1, Led::Button, led_color, BUTTON_BRIGHTNESS);
                    }
                }
                leds.unset(1, Led::Top);
            }

            old_gatein = gatein;
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(1).await;

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
                leds.set(1, Led::Button, led_color, Brightness::Lower);
            }
        }
    };
    let fut3 = async {
        loop {
            let chan = fader.wait_for_any_change().await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    if storage.query(|s| s.muted) {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Lower);
                    }

                    muted_glob.set(storage.query(|s| s.muted));
                    att_glob.set(storage.query(|s| s.att_saved));
                    offset_glob.set(storage.query(|s| s.offset_saved));
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
