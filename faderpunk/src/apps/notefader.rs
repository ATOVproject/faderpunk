use defmt::info;
use embassy_futures::{join::join5, select::select};

use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    colors::RED, ext::FromValue, latch::LatchLayer, utils::is_close, Brightness, Color, Config,
    Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

const LED_BRIGHTNESS: Brightness = Brightness::Lower;

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
            Color::Pink,
            Color::Cyan,
            Color::Red,
            Color::White,
        ],
    });

pub struct Params {
    midi_channel: i32,
    note: i32,
    span: i32,
    gatel: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: 1,
            note: 48,
            span: 24,
            gatel: 50,
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
            span: i32::from_value(values[2]),
            gatel: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note.into()).unwrap();
        vec.push(self.span.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    note_saved: u16,
    fader_saved: u16,
    mute_saved: bool,
    clocked: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            note_saved: 0,
            fader_saved: 3000,
            mute_saved: false,
            clocked: false,
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
    let range = Range::_0_10V;
    let (midi_chan, gatel, base_note, span, led_color) =
        params.query(|p| (p.midi_channel, p.gatel, p.note, p.span, p.color));

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
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    let (res, mute, att) = storage.query(|s| (s.fader_saved, s.mute_saved, s.clocked));

    clocked_glob.set(att);
    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
    }

    let trigger_note = async |_| {
        let fadval = (storage.query(|s| (s.note_saved)) as i32 * (span + 3) / 120) as u16;

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
                    let muted = glob_muted.get();

                    let div = div_glob.get();

                    if clkn % div == 0 && clocked_glob.get() {
                        if !muted {
                            if note_on {
                                midi.send_note_off(note as u8).await;
                            }
                            note = trigger_note(note).await;
                            note_on = true;
                        }

                        if buttons.is_shift_pressed() {
                            leds.set(0, Led::Bottom, Color::Red, LED_BRIGHTNESS);
                        }
                    }

                    if clkn % div == (div * gatel / 100).clamp(1, div - 1) {
                        if note_on {
                            midi.send_note_off(note as u8).await;
                            leds.set(0, Led::Top, led_color.into(), Brightness::Custom(0));
                            note_on = false;
                        }

                        leds.set(0, Led::Bottom, led_color.into(), Brightness::Custom(0));
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
                    leds.unset_all();
                } else {
                    leds.set(0, Led::Button, led_color.into(), LED_BRIGHTNESS);
                }
            } else {
                let mode = clocked_glob.toggle();
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
        let mut latch = app.make_latch(fader.get_value());

        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.note_saved),
                LatchLayer::Alt => storage.query(|s| s.fader_saved),
                _ => unreachable!(),
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage
                            .modify_and_save(|s| s.note_saved = new_value, None)
                            .await;
                    }
                    LatchLayer::Alt => {
                        div_glob.set(resolution[new_value as usize / 345]);
                        storage
                            .modify_and_save(|s| s.fader_saved = new_value, None)
                            .await;
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
                    let (res, mute, clk) =
                        storage.query(|s| (s.fader_saved, s.mute_saved, s.clocked));

                    clocked_glob.set(clk);
                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.set(0, Led::Button, led_color.into(), Brightness::Lowest);

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
    let mut button_old = false;

    let shift = async {
        let mut note = 62;
        loop {
            // latching on pressing and depressing shift
            app.delay_millis(1).await;

            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false);
                let base: u8 = LED_BRIGHTNESS.into();
                leds.set(
                    0,
                    Led::Bottom,
                    Color::Red,
                    Brightness::Custom(base * clocked_glob.get() as u8),
                );
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false);
                shift_old = false;
            }

            // use this to trigger notes
            if !clocked_glob.get() && !buttons.is_shift_pressed() {
                if !button_old && buttons.is_button_pressed(0) {
                    button_old = true;
                    note = trigger_note(note).await;
                }
                if button_old && !buttons.is_button_pressed(0) {
                    button_old = false;
                    midi.send_note_off(note as u8).await;
                    leds.set(0, Led::Top, led_color.into(), Brightness::Custom(0));
                    leds.set(0, Led::Button, led_color.into(), Brightness::Lowest);
                }
            }
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
