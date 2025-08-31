use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    colors::RED, ext::FromValue, utils::is_close, Brightness, Color, Config, Curve, Param, Value,
    APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

const LED_BRIGHTNESS: Brightness = Brightness::Lower;

pub static CONFIG: Config<PARAMS> =
    Config::new("Random Triggers", "Generate random triggers on clock")
        .add_param(Param::i32 {
            name: "MIDI Channel",
            min: 1,
            max: 16,
        })
        .add_param(Param::i32 {
            name: "MIDI NOTE",
            min: 1,
            max: 128,
        })
        .add_param(Param::i32 {
            name: "GATE %",
            min: 1,
            max: 100,
        })
        .add_param(Param::Curve {
            name: "Fader Curve",
            variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
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

pub struct Params {
    midi_channel: i32,
    note: i32,
    gatel: i32,
    curve: Curve,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: 1,
            note: 32,
            gatel: 50,
            curve: Curve::Linear,
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
            midi_channel: i32::from_value(values[0]),
            note: i32::from_value(values[1]),
            gatel: i32::from_value(values[2]),
            curve: Curve::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.curve.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_saved: bool,
    prob_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_saved: false,
            prob_saved: 4096,
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
    let (midi_chan, note, gatel, led_color, curve) =
        params.query(|p| (p.midi_channel, p.note, p.gatel, p.color, p.curve));

    let mut clock = app.use_clock();
    let die = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let latched_glob = app.make_global(false);
    let prob_glob = app.make_global(4095);

    let jack = app.make_gate_jack(0, 4095).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    // const led_color.into(): RGB<u8> = ATOV_YELLOW;

    let mut rndval = die.roll();

    let (res, mute, att) = storage.query(|s| (s.fader_saved, s.mute_saved, s.prob_saved));

    prob_glob.set(att);
    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
    }

    let fut1 = async {
        let mut note_on = false;

        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    midi.send_note_off(note as u8 - 1).await;
                    note_on = false;
                    jack.set_low().await;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();
                    let val = prob_glob.get();
                    let div = div_glob.get();

                    if clkn % div == 0 {
                        if curve.at(val) >= rndval && !muted {
                            jack.set_high().await;
                            leds.set(0, Led::Top, led_color.into(), LED_BRIGHTNESS);
                            midi.send_note_on(note as u8 - 1, 4095).await;
                            note_on = true;
                        }

                        if buttons.is_shift_pressed() {
                            leds.set(0, Led::Bottom, RED, LED_BRIGHTNESS);
                        }
                        rndval = die.roll();
                    }

                    if clkn % div == (div * gatel / 100).clamp(1, div - 1) {
                        if note_on {
                            midi.send_note_off(note as u8 - 1).await;
                            leds.unset(0, Led::Top);
                            note_on = false;
                            jack.set_low().await;
                        }

                        leds.unset(0, Led::Bottom);
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
            let muted = glob_muted.toggle();

            storage
                .modify_and_save(
                    |s| {
                        s.mute_saved = muted;
                        s.mute_saved
                    },
                    None,
                )
                .await;

            if muted {
                jack.set_low().await;
                leds.unset_all();
            } else {
                leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change_at(0).await;
            storage.load(None).await;
            let fad = fader.get_value();

            if buttons.is_shift_pressed() {
                let fad_saved = storage.query(|s| s.fader_saved);
                if is_close(fad, fad_saved) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    div_glob.set(resolution[fad as usize / 345]);
                    storage.modify_and_save(|s| s.fader_saved = fad, None).await;
                }
            } else {
                let prob = prob_glob.get();
                if is_close(fad, prob) {
                    latched_glob.set(true);
                }
                if latched_glob.get() {
                    prob_glob.set(fad);
                    storage.modify_and_save(|s| s.prob_saved = fad, None).await;
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
                        storage.query(|s| (s.fader_saved, s.mute_saved, s.prob_saved));

                    prob_glob.set(att);
                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.unset(0, Led::Button);
                        jack.set_low().await;
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
                    }
                    latched_glob.set(false);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    let mut shift_old = false;

    let shift = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;
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

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
