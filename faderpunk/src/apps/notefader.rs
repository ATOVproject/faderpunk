use defmt::info;
use embassy_futures::{join::join5, select::select};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use libfp::{colors::RED, utils::is_close, Brightness, Color, Config, Param, Range, Value};

use crate::app::{
    App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

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

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    note: ParamSlot<'a, i32, PARAMS>,
    span: ParamSlot<'a, i32, PARAMS>,
    gatel: ParamSlot<'a, i32, PARAMS>,
    color: ParamSlot<'a, Color, PARAMS>,
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
            Value::Color(Color::Yellow),
        ],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel: ParamSlot::new(&param_store, 0),
        note: ParamSlot::new(&param_store, 1),
        span: ParamSlot::new(&param_store, 2),
        gatel: ParamSlot::new(&param_store, 3),
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
    let range = Range::_0_10V;
    let midi_chan = params.midi_channel.get().await;
    let gatel = params.gatel.get().await;
    let base_note = params.note.get().await;
    let span = params.span.get().await;

    let mut clock = app.use_clock();
    let quantizer = app.use_quantizer(range);

    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_chan as u8 - 1);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let latched_glob = app.make_global(false);
    let clocked_glob = app.make_global(false);

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    const LED_BRIGHTNESS: Brightness = Brightness::Lower;

    // const led_color.into(): RGB<u8> = ATOV_YELLOW;
    let led_color = params.color.get().await;

    let (res, mute, att) = storage
        .query(|s| (s.fader_saved, s.mute_saved, s.clocked))
        .await;

    clocked_glob.set(att).await;
    glob_muted.set(mute).await;
    div_glob.set(resolution[res as usize / 345]).await;
    if mute {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
    }

    let trigger_note = async |_| {
        let fadval = (fader.get_value() as i32 * (span + 3) / 120) as u16;

        leds.set(0, Led::Top, led_color.into(), LED_BRIGHTNESS);

        let out = quantizer.get_quantized_note(fadval).await;

        info!(
            "bit : {}, voltage: {}, corrected out: {}",
            fadval,
            out.as_v_oct(),
            out.as_counts(range)
        );

        jack.set_value(out.as_counts(range));
        let note = out.as_midi() as i32 + base_note;
        midi.send_note_on(note as u8, 4095).await;
        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
        note as u16
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
                            leds.unset(0, Led::Top);
                            note_on = false;
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
                    leds.unset_all();
                } else {
                    leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
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
                        leds.set(0, Led::Button, led_color.into(), Brightness::Lowest);

                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
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
    let mut button_old = false;

    let shift = async {
        let mut note = 62;
        loop {
            // latching on pressing and depressing shift
            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                let base: u8 = LED_BRIGHTNESS.into();
                leds.set(
                    0,
                    Led::Bottom,
                    RED,
                    Brightness::Custom(base * clocked_glob.get().await as u8),
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
                    leds.unset(0, Led::Top);
                    leds.set(0, Led::Button, led_color.into(), Brightness::Lowest);
                }
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
