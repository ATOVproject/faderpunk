use config::{Config, Param, Value};
use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppStorage, Arr, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, Range, SceneEvent,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

pub static CONFIG: config::Config<PARAMS> = Config::new("Automator", "Fader movement recording")
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

#[derive(Serialize, Deserialize)]
pub struct Storage {
    buffer_saved: Arr<u16, 384>,
    length_saved: usize,
}

impl AppStorage for Storage {}

impl Default for Storage {
    fn default() -> Self {
        Self {
            buffer_saved: Arr::new([0; 384]),
            length_saved: 384,
        }
    }
}

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    cc: ParamSlot<'a, i32, PARAMS>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new(
        [Value::i32(1), Value::i32(32)],
        app.app_id,
        app.start_channel,
    );

    let params = Params {
        midi_channel: ParamSlot::new(&param_store, 0),
        cc: ParamSlot::new(&param_store, 1),
    };

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let cc = params.cc.get().await;
    let midi = app.use_midi(midi_chan as u8 - 1);
    let mut clock = app.use_clock();

    let rec_flag = app.make_global(false);
    let offset_glob = app.make_global(0);
    let buffer_glob = app.make_global([0; 384]);
    let recording_glob = app.make_global(false);
    let length_glob = app.make_global(384);
    let index_glob = app.make_global(0);
    let latched = app.make_global(false);

    let jack = app.make_out_jack(0, Range::_0_10V).await;

    let mut last_midi = 0;

    let mut index = 0;
    let mut recording = false;
    let mut buffer = [0; 384];
    let mut length = 384;
    let color = (255, 255, 255);

    let (buffer_saved, length_saved) = storage
        .query(|s| (s.buffer_saved.get(), s.length_saved))
        .await;

    buffer_glob.set(buffer_saved).await;
    length_glob.set(length_saved).await;

    leds.set(0, Led::Button, (255, 255, 255), 100);

    let update_output = async {
        loop {
            app.delay_millis(1).await;
            let index = index_glob.get().await;
            let buffer = buffer_glob.get().await;
            let offset = offset_glob.get().await;

            if recording_glob.get().await {
                jack.set_value(offset);
                if last_midi / 16 != (offset) / 16 {
                    midi.send_cc(0, offset).await;
                    last_midi = offset;
                }
                leds.set(0, Led::Top, (255, 0, 0), (offset / 32) as u8);
                leds.set(0, Led::Bottom, (255, 0, 0), (255 - (offset / 16) as u8) / 2)
            } else {
                let mut val = buffer[index] + offset;
                val = val.clamp(0, 4095);
                jack.set_value(val);
                if last_midi / 16 != (val) / 16 {
                    midi.send_cc(cc as u8, val).await;
                    last_midi = val;
                }
                leds.set(0, Led::Top, color, (val / 32) as u8);
                leds.set(0, Led::Bottom, color, (255 - (val / 16) as u8) / 2)
            }
        }
    };

    let fut1 = async {
        loop {
            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    index = 0;
                    recording = false;
                    recording_glob.set(recording).await;
                }
                ClockEvent::Tick => {
                    index += 1;
                }
                _ => {}
            }

            length = length_glob.get().await;

            index = index % length;

            index_glob.set(index).await;
            recording = recording_glob.get().await;

            if index == 0 && recording {
                //stop recording at max length
                recording = false;
                recording_glob.set(recording).await;
                length = 384;
                length_glob.set(length).await;
                buffer_glob.set(buffer).await;
                offset_glob.set(0).await;
                latched.set(false).await;
                storage
                    .modify(|s| {
                        s.buffer_saved.set(buffer);
                        s.length_saved = length;
                    })
                    .await;
            }

            if rec_flag.get().await && index % 96 == 0 {
                index = 0;
                recording = true;
                recording_glob.set(recording).await;
                rec_flag.set(false).await;
                length = 384;
                length_glob.set(length).await;
                latched.set(true).await
            }

            if recording {
                let val = faders.get_values();
                buffer[index] = val[0];
                leds.set(0, Led::Button, (255, 0, 0), 100);
            } else {
                leds.set(0, Led::Button, color, 100);
            }

            if recording && !buttons.is_button_pressed(0) && index % 96 == 0 && index != 0 {
                //finish recording
                recording = !recording;
                recording_glob.set(recording).await;
                length = index;
                length_glob.set(length).await;
                buffer_glob.set(buffer).await;
                offset_glob.set(0).await;
                latched.set(false).await;
                storage
                    .modify(|s| {
                        s.buffer_saved.set(buffer);
                        s.length_saved = length;
                    })
                    .await;
            }

            if index == 1 {
                leds.set(0, Led::Button, (255, 255, 255), 0);
            }
        }
    };

    let fut2 = async {
        loop {
            faders.wait_for_change(0).await;
            let val = faders.get_values();

            if is_close(val[0], offset_glob.get().await) && !latched.get().await {
                latched.set(true).await
            }
            if latched.get().await {
                offset_glob.set(val[0]).await;
            }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            if buttons.is_shift_pressed() {
                recording_glob.set(false).await;
                buffer_glob.set([0; 384]).await;
                length_glob.set(384).await;
                leds.set(0, Led::Button, color, 100);
                latched.set(false).await;
            } else {
                rec_flag.set(true).await;
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (buffer_saved, length_saved) = storage
                        .query(|s| (s.buffer_saved.get(), s.length_saved))
                        .await;
                    buffer_glob.set(buffer_saved).await;
                    length_glob.set(length_saved).await;
                    latched.set(false).await;
                    offset_glob.set(0).await;
                }
                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;
                }
            }
        }
    };

    join5(update_output, fut1, fut2, fut3, scene_handler).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
