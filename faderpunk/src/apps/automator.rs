//Bug :
// No midi out when recording

use embassy_futures::{join::join4, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use libfp::utils::slew_limiter;
use serde::{Deserialize, Serialize};
use smart_leds::colors::RED;

use libfp::{Config, Param, Value};

use crate::{
    app::{
        colors::WHITE, App, AppStorage, Arr, ClockEvent, Led, ManagedStorage, ParamSlot, Range,
        SceneEvent, RGB8,
    },
    apps::slew,
    storage::ParamStore,
};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

pub static CONFIG: Config<PARAMS> = Config::new("Automator", "Fader movement recording")
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

pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    cc: ParamSlot<'a, i32, PARAMS>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            buffer_saved: Arr::new([0; 384]),
            length_saved: 384,
        }
    }
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
    let fader = app.use_faders();
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
    let slew_rate = 1000;

    // let (buffer_saved, length_saved) = storage
    //     .query(|s| (s.buffer_saved.get(), s.length_saved))
    //     .await;
    // buffer_glob.set(buffer_saved).await;
    // length_glob.set(length_saved).await;

    leds.set(0, Led::Button, WHITE, 100);

    let update_output = async {
        let mut outval = 0.;
        loop {
            app.delay_millis(1).await;
            let index = index_glob.get().await;
            let buffer = buffer_glob.get().await;
            let mut offset = offset_glob.get().await;
            if latched.get().await {
                offset = fader.get_value();
            }

            // if recording_glob.get().await {
            //     jack.set_value(offset);
            //     info!("last midi {}, offset {}", last_midi / 16, (offset) / 16 );
            //     if last_midi / 16 != (offset) / 16 {
            //         midi.send_cc(0, offset).await;
            //         last_midi = offset;
            //     }
            //     leds.set(0, Led::Top, (255, 0, 0), (offset / 32) as u8);
            //     leds.set( 0,Led::Bottom,(255, 0, 0),(255 - (offset/ 16) as u8) / 2)

            let val = buffer[index] + offset;
            outval = slew_limiter(outval, val, slew_rate, slew_rate);
            jack.set_value(outval as u16);
            if last_midi / 16 != (val) / 16 {
                midi.send_cc(cc as u8, outval as u16).await;
                last_midi = outval as u16;
            };
            if recording_glob.get().await {
                leds.set(0, Led::Top, RED, (outval / 16.) as u8);
            } else {
                leds.set(0, Led::Top, WHITE, (outval / 16.) as u8)
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
                    length = length_glob.get().await;

                    index %= length;

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
                        // storage
                        //     .modify(|s| {
                        //         s.buffer_saved.set(buffer);
                        //         s.length_saved = length;
                        //     })
                        //     .await;
                    }

                    if rec_flag.get().await && index % 96 == 0 {
                        index = 0;
                        recording = true;
                        buffer = [0; 384];
                        buffer_glob.set(buffer).await;
                        recording_glob.set(recording).await;
                        rec_flag.set(false).await;
                        length = 384;
                        length_glob.set(length).await;
                        latched.set(true).await
                    }

                    if recording {
                        let val = fader.get_value();
                        buffer[index] = val;
                        leds.set(0, Led::Button, RED, 100);
                    } else {
                        leds.set(0, Led::Button, WHITE, 100);
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
                        // storage
                        //     .modify(|s| {
                        //         s.buffer_saved.set(buffer);
                        //         s.length_saved = length;
                        //     })
                        //     .await;
                    }

                    if index == 0 {
                        leds.reset(0, Led::Button);
                    }

                    index += 1;
                }
                _ => {}
            }
        }
    };

    let fut2 = async {
        loop {
            fader.wait_for_change().await;
            let val = fader.get_value();

            if is_close(val, offset_glob.get().await) && !latched.get().await {
                latched.set(true).await
            }
            // if latched.get().await {
            //     offset_glob.set(val).await;
            // }
        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            if buttons.is_shift_pressed() {
                recording_glob.set(false).await;
                buffer_glob.set([0; 384]).await;
                length_glob.set(384).await;
                leds.set(0, Led::Button, WHITE, 100);
                latched.set(false).await;
            } else {
                rec_flag.set(true).await;
            }
        }
    };

    // let scene_handler = async {
    //     loop {
    //         match app.wait_for_scene_event().await {
    //             SceneEvent::LoadSscene(scene) => {
    //                 storage.load(Some(scene)).await;
    //                 let (buffer_saved, length_saved) = storage
    //                     .query(|s| (s.buffer_saved.get(), s.length_saved))
    //                     .await;
    //                 buffer_glob.set(buffer_saved).await;
    //                 length_glob.set(length_saved).await;
    //                 latched.set(false).await;
    //                 offset_glob.set(0).await;
    //             }
    //             SceneEvent::SaveScene(scene) => {
    //                 storage.save(Some(scene)).await;
    //             }
    //         }
    //     }
    // };

    join4(update_output, fut1, fut2, fut3).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
