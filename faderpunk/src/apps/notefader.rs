use defmt::info;
use embassy_futures::{join::join5, select::select};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};
use smart_leds::{colors::RED, RGB};

use libfp::{
    constants::{ATOV_RED, ATOV_YELLOW, LED_LOW, LED_MID},
    quantizer::{Key, Note},
    utils::is_close,
    Config, Param, Range, Value,
};

use crate::app::{
    App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 4;

pub static CONFIG: Config<PARAMS> = Config::new("Note Fader", "Play notes manually or on clock")
    .add_param(Param::i32 {
        name: "MIDI Channel",
        min: 1,
        max: 16,
    })
    .add_param(Param::i32 {
        name: "Base note",
        min: 1,
        max: 128,
    })
    .add_param(Param::i32 {
        name: "Span",
        min: 1,
        max: 120,
    })
    .add_param(Param::i32 {
        name: "GATE %",
        min: 1,
        max: 100,
    });

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    note: ParamSlot<'a, i32, PARAMS>,
    span: ParamSlot<'a, i32, PARAMS>,
    gatel: ParamSlot<'a, i32, PARAMS>,
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_saved: bool,
    clocked: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_saved: false,
            clocked: false,
        }
    }
}
impl AppStorage for Storage {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [
            Value::i32(1),
            Value::i32(48),
            Value::i32(24),
            Value::i32(50),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel: ParamSlot::new(&param_store, 0),
        note: ParamSlot::new(&param_store, 1),
        span: ParamSlot::new(&param_store, 2),
        gatel: ParamSlot::new(&param_store, 3),
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
    let mut quantizer = app.use_quantizer();

    quantizer.set_scale(Key::PentatonicMinor, Note::C, Note::C);

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let gatel = params.gatel.get().await;
    let base_note = params.note.get().await;
    let span = params.span.get().await;

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let latched_glob = app.make_global(false);
    let clocked_glob = app.make_global(false);

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    const LED_BRIGHTNESS: u8 = LED_MID;

    const LED_COLOR: RGB<u8> = ATOV_YELLOW;

    let (res, mute, att) = storage
        .query(|s| (s.fader_saved, s.mute_saved, s.clocked))
        .await;

    clocked_glob.set(att).await;
    glob_muted.set(mute).await;
    div_glob.set(resolution[res as usize / 345]).await;
    if mute {
        leds.set(0, Led::Button, LED_COLOR, 0);
        leds.set(0, Led::Top, LED_COLOR, 0);
        leds.set(0, Led::Bottom, LED_COLOR, 0);
    } else {
        leds.set(0, Led::Button, LED_COLOR, LED_BRIGHTNESS);
    }

    let trigger_note = async |_| {
        let fadval = (fader.get_value() as i32 * (span + 3) / 120) as u16;

        leds.set(0, Led::Top, LED_COLOR, LED_BRIGHTNESS);

        let out = ((quantizer.get_quantized_voltage(fadval)) * 410.) as u16;

        info!(
            "bit : {}, voltage: {}, corrected out: {}",
            fadval,
            quantizer.get_quantized_voltage(fadval),
            out
        );

        jack.set_value(out);
        let note = (out as u32 * 120 / 4095 + base_note as u32) as u16;
        midi.send_note_on(note as u8, 4095).await;
        leds.set(0, Led::Button, LED_COLOR, LED_BRIGHTNESS);
        note
    };

    let fut1 = async {
        let mut note_on = false;
        let mut note = 0;

        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    midi.send_note_off(note as u8).await;
                    note_on = false;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get().await;

                    let div = div_glob.get().await;

                    if clkn % div == 0 && clocked_glob.get().await {
                        if !muted {
                            if note_on {
                                midi.send_note_off(note as u8).await;
                            }
                            note = trigger_note(note).await;
                            note_on = true;
                        }

                        if buttons.is_shift_pressed() {
                            leds.set(0, Led::Bottom, RED, LED_BRIGHTNESS);
                        }
                    }

                    if clkn % div == (div * gatel / 100).clamp(1, div - 1) {
                        if note_on {
                            midi.send_note_off(note as u8).await;
                            leds.set(0, Led::Top, LED_COLOR, 0);
                            note_on = false;
                        }

                        leds.set(0, Led::Bottom, ATOV_RED, 0);
                    }
                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;
            if !buttons.is_shift_pressed() {
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
                    leds.reset_all();
                } else {
                    leds.set(0, Led::Button, LED_COLOR, LED_BRIGHTNESS);
                }
            } else {
                let mode = clocked_glob.toggle().await;
                storage
                    .modify_and_save(
                        |s| {
                            s.clocked = mode;
                            s.clocked
                        },
                        None,
                    )
                    .await;
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
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (res, mute, clk) = storage
                        .query(|s| (s.fader_saved, s.mute_saved, s.clocked))
                        .await;

                    clocked_glob.set(clk).await;
                    glob_muted.set(mute).await;
                    div_glob.set(resolution[res as usize / 345]).await;
                    if mute {
                        leds.set(0, Led::Button, LED_COLOR, LED_LOW);

                        leds.set(0, Led::Top, LED_COLOR, 0);
                        leds.set(0, Led::Bottom, LED_COLOR, 0);
                    } else {
                        leds.set(0, Led::Button, LED_COLOR, LED_BRIGHTNESS);
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
    let mut button_old = false;

    let shift = async {
        let mut note = 62;
        loop {
            // latching on pressing and depressing shift
            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                leds.set(
                    0,
                    Led::Bottom,
                    ATOV_RED,
                    LED_MID * clocked_glob.get().await as u8,
                );
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = false;
            }

            // use this to trigger notes
            if !clocked_glob.get().await && !buttons.is_shift_pressed() {
                if !button_old && buttons.is_button_pressed(0) {
                    button_old = true;
                    note = trigger_note(note).await;
                }
                if button_old && !buttons.is_button_pressed(0) {
                    button_old = false;
                    midi.send_note_off(note as u8).await;
                    leds.set(0, Led::Top, LED_COLOR, 0);
                    leds.set(0, Led::Button, LED_COLOR, LED_LOW);
                }
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
