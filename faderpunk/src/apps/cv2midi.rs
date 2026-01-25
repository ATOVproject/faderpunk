use embassy_futures::{
    join::join4,
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    latch::LatchLayer,
    utils::{attenuate, attenuate_bipolar, split_unsigned_value},
    AppIcon, Brightness, Color, MidiCc, MidiChannel, MidiOut, APP_MAX_PARAMS,
};
use serde::{Deserialize, Serialize};

use libfp::{ext::FromValue, Config, Param, Range, Value};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 5;

const BUTTON_BRIGHTNESS: Brightness = Brightness::Mid;

pub static CONFIG: Config<PARAMS> = Config::new(
    "CV to MIDI",
    "CV to MIDI CC",
    Color::Violet,
    AppIcon::NoteGrid,
)
.add_param(Param::bool { name: "Bipolar" })
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiCc { name: "MIDI CC" })
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
    bipolar: bool,
    midi_channel: MidiChannel,
    midi_cc: MidiCc,
    midi_out: MidiOut,
    color: Color,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            bipolar: false,
            midi_channel: MidiChannel::default(),
            midi_cc: MidiCc::from(32),
            midi_out: MidiOut::default(),
            color: Color::Violet,
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
            midi_channel: MidiChannel::from_value(values[1]),
            midi_cc: MidiCc::from_value(values[2]),
            color: Color::from_value(values[3]),
            midi_out: MidiOut::from_value(values[4]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.bipolar.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.midi_cc.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

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
    let (bipolar, midi_out, midi_channel, midi_cc, led_color) =
        params.query(|p| (p.bipolar, p.midi_out, p.midi_channel, p.midi_cc, p.color));

    let buttons = app.use_buttons();
    let fader = app.use_faders();
    let leds = app.use_leds();

    let midi = app.use_midi_output(midi_out, midi_channel);

    let muted_glob = app.make_global(false);

    let glob_latch_layer = app.make_global(LatchLayer::Main);

    muted_glob.set(storage.query(|s| s.muted));

    if storage.query(|s| s.muted) {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, BUTTON_BRIGHTNESS);
    }

    let input = if bipolar {
        app.make_in_jack(0, Range::_Neg5_5V).await
    } else {
        app.make_in_jack(0, Range::_0_10V).await
    };

    let fut1 = async {
        let mut old_midi = 0;

        loop {
            app.delay_millis(1).await;
            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let input_val = if !muted_glob.get() {
                if !bipolar {
                    (attenuate(input.get_value() * 2, storage.query(|s| s.att_saved))
                        + storage.query(|s| s.offset_saved))
                    .clamp(0, 4095)
                } else {
                    (attenuate_bipolar(input.get_value(), storage.query(|s| s.att_saved)) as i16
                        + (storage.query(|s| s.offset_saved) as i16 - 2047))
                        .clamp(0, 4095) as u16
                }
            } else if bipolar {
                2047
            } else {
                0
            };
            if latch_active_layer == LatchLayer::Main {
                if bipolar {
                    let led1 = split_unsigned_value(input_val);
                    leds.set(0, Led::Top, led_color, Brightness::Custom(led1[0]));
                    leds.set(0, Led::Bottom, led_color, Brightness::Custom(led1[1]));
                } else {
                    leds.set(
                        0,
                        Led::Top,
                        led_color,
                        Brightness::Custom((input_val / 16) as u8),
                    );
                }
            } else {
                leds.set(
                    0,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((storage.query(|s| s.att_saved) / 16) as u8),
                );
            }

            if old_midi != input_val / 32 {
                midi.send_cc(midi_cc, input_val).await;
                old_midi = input_val / 32;
            }
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_down(0).await;

            let muted = storage.modify_and_save(|s| {
                s.muted = !s.muted;
                s.muted
            });
            muted_glob.set(muted);
            if muted {
                leds.unset(0, Led::Button);
            } else {
                leds.set(0, Led::Button, led_color, Brightness::Mid);
            }
        }
    };
    let fut3 = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.offset_saved),
                LatchLayer::Alt => storage.query(|s| s.att_saved),
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.offset_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.att_saved = new_value);
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

                    if storage.query(|s| s.muted) {
                        leds.unset(0, Led::Button);
                    } else {
                        leds.set(0, Led::Button, led_color, Brightness::Mid);
                    }

                    muted_glob.set(storage.query(|s| s.muted));
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}
