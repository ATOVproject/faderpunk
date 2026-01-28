use embassy_futures::{
    join::join5,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, AppIcon, Brightness, ClockDivision, Color, Config, Curve,
    MidiChannel, MidiNote, MidiOut, Param, Value, APP_MAX_PARAMS,
};

use crate::app::{
    App, AppParams, AppStorage, ClockEvent, Led, ManagedStorage, ParamStore, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

const LED_BRIGHTNESS: Brightness = Brightness::High;

pub static CONFIG: Config<PARAMS> = Config::new(
    "Random Triggers",
    "Generate random triggers on clock",
    Color::Cyan,
    AppIcon::Die,
)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote { name: "MIDI Note" })
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    midi_note: MidiNote,
    midi_out: MidiOut,
    gatel: i32,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            midi_note: MidiNote::from(32),
            midi_out: MidiOut([false, false, false]),
            gatel: 50,
            color: Color::Cyan,
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_channel: MidiChannel::from_value(values[0]),
            midi_note: MidiNote::from_value(values[1]),
            gatel: i32::from_value(values[2]),
            color: Color::from_value(values[3]),
            midi_out: MidiOut::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_note.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
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
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

    let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let (midi_out, midi_chan, note, gatel, led_color) =
        params.query(|p| (p.midi_out, p.midi_channel, p.midi_note, p.gatel, p.color));
    let curve = Curve::Exponential;

    let mut clock = app.use_clock();
    let die = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_chan);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let jack = app.make_gate_jack(0, 4095).await;

    let resolution = [384, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];

    let mut clkn = 0;

    let mut rndval = die.roll();

    let (res, mute) = storage.query(|s| (s.fader_saved, s.mute_saved));

    glob_muted.set(mute);
    div_glob.set(resolution[res as usize / 345]);
    if mute {
        leds.unset(0, Led::Button);
        leds.unset(0, Led::Top);
        leds.unset(0, Led::Bottom);
    } else {
        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
    }

    let fut1 = async {
        let mut note_on = false;

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                    midi.send_note_off(note).await;
                    note_on = false;
                    jack.set_low().await;
                }
                ClockEvent::Tick => {
                    let muted = glob_muted.get();
                    let val = storage.query(|s| s.prob_saved);
                    let div = div_glob.get();

                    if clkn % div == 0 {
                        if curve.at(val) >= rndval && !muted {
                            jack.set_high().await;
                            leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
                            midi.send_note_on(note, 4095).await;
                            note_on = true;
                        }

                        if glob_latch_layer.get() == LatchLayer::Alt {
                            leds.set(0, Led::Bottom, Color::Red, LED_BRIGHTNESS);
                        } else {
                            leds.unset(0, Led::Bottom);
                        }
                        rndval = die.roll();
                    }

                    if clkn % div == (div * gatel / 100).clamp(1, div - 1) {
                        if note_on {
                            midi.send_note_off(note).await;
                            leds.set(0, Led::Top, led_color, Brightness::Off);
                            note_on = false;
                            jack.set_low().await;
                        }

                        leds.set(0, Led::Bottom, led_color, Brightness::Off);
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

            storage.modify_and_save(|s| {
                s.mute_saved = muted;
                s.mute_saved
            });

            if muted {
                jack.set_low().await;
                leds.unset_all();
            } else {
                leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
            }
        }
    };

    let fut3 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change_at(0).await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Alt => storage.query(|s| s.fader_saved),
                LatchLayer::Main => storage.query(|s| s.prob_saved),
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Alt => {
                        div_glob.set(resolution[new_value as usize / 345]);
                        storage.modify_and_save(|s| s.fader_saved = new_value);
                    }
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.prob_saved = new_value);
                    }
                    LatchLayer::Third => {}
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    let (res, mute, _att) =
                        storage.query(|s| (s.fader_saved, s.mute_saved, s.prob_saved));

                    glob_muted.set(mute);
                    div_glob.set(resolution[res as usize / 345]);
                    if mute {
                        leds.unset(0, Led::Button);
                        jack.set_low().await;
                        leds.unset(0, Led::Top);
                        leds.unset(0, Led::Bottom);
                    } else {
                        leds.set(0, Led::Button, led_color, LED_BRIGHTNESS);
                    }
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    let shift = async {
        loop {
            app.delay_millis(1).await;
            glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));
        }
    };

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}
