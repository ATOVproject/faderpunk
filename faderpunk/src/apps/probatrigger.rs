use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::{
    constants::{ATOV_YELLOW, LED_MID},
    utils::is_close,
    Color, Config, Curve, Param, Value,
};
use serde::{Deserialize, Serialize};
use smart_leds::{colors::RED, RGB};

use crate::app::{
    App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

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
                Color::Blue,
                Color::Red,
                Color::White,
            ],
        });

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    note: ParamSlot<'a, i32, PARAMS>,
    gatel: ParamSlot<'a, i32, PARAMS>,
    curve: ParamSlot<'a, Curve, PARAMS>,
    color: ParamSlot<'a, Color, PARAMS>,
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
    let param_store = ParamStore::new(
        [
            Value::i32(1),
            Value::i32(32),
            Value::i32(50),
            Value::Curve(Curve::Linear),
            Value::Color(Color::Yellow),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel: ParamSlot::new(&param_store, 0),
        note: ParamSlot::new(&param_store, 1),
        gatel: ParamSlot::new(&param_store, 2),
        curve: ParamSlot::new(&param_store, 3),
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
    let mut clock = app.use_clock();
    let die = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let note = params.note.get().await;
    let gatel = params.gatel.get().await;
    let midi = app.use_midi_output(midi_chan as u8 - 1);
    let led_color = params.color.get().await;

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let latched_glob = app.make_global(false);
    let prob_glob = app.make_global(4095);

    let jack = app.make_gate_jack(0, 4095).await;

    let curve = params.curve.get().await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    const LED_BRIGHTNESS: u8 = LED_MID;

    // const led_color.into(): RGB<u8> = ATOV_YELLOW;

    let mut rndval = die.roll();

    let (res, mute, att) = storage
        .query(|s| (s.fader_saved, s.mute_saved, s.prob_saved))
        .await;

    prob_glob.set(att).await;
    glob_muted.set(mute).await;
    div_glob.set(resolution[res as usize / 345]).await;
    if mute {
        leds.set(0, Led::Button, led_color.into(), 0);
        leds.set(0, Led::Top, led_color.into(), 0);
        leds.set(0, Led::Bottom, led_color.into(), 0);
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
                    let muted = glob_muted.get().await;
                    let val = prob_glob.get().await;
                    let div = div_glob.get().await;

                    if clkn % div == 0 {
                        if curve.at(val as usize) >= rndval && !muted {
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
                            leds.set(0, Led::Top, led_color.into(), 0);
                            note_on = false;
                            jack.set_low().await;
                        }

                        leds.set(0, Led::Bottom, RED, 0);
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
            let muted = glob_muted.toggle().await;

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
                leds.reset_all();
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
                let fad_saved = storage.query(|s| s.fader_saved).await;
                if is_close(fad, fad_saved) {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    div_glob.set(resolution[fad as usize / 345]).await;
                    storage.modify_and_save(|s| s.fader_saved = fad, None).await;
                }
            } else {
                let prob = prob_glob.get().await;
                if is_close(fad, prob) {
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    prob_glob.set(fad).await;
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
                    let (res, mute, att) = storage
                        .query(|s| (s.fader_saved, s.mute_saved, s.prob_saved))
                        .await;

                    prob_glob.set(att).await;
                    glob_muted.set(mute).await;
                    div_glob.set(resolution[res as usize / 345]).await;
                    if mute {
                        leds.set(0, Led::Button, led_color.into(), 0);
                        jack.set_low().await;
                        leds.set(0, Led::Top, led_color.into(), 0);
                        leds.set(0, Led::Bottom, led_color.into(), 0);
                    } else {
                        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
                    }
                    latched_glob.set(false).await;
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
                latched_glob.set(false).await;
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = false;
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
