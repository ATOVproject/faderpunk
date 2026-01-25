use embassy_futures::{
    join::{join4, join5},
    select::{select, select3},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use midly::MidiMessage;
use serde::{Deserialize, Serialize};

use libfp::{
    ext::FromValue, latch::LatchLayer, utils::attenuate, AppIcon, Brightness, Color, Config, Curve,
    MidiChannel, MidiIn, Param, Range, Value, APP_MAX_PARAMS,
};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 3;

pub static CONFIG: Config<PARAMS> = Config::new(
    "AD Envelope",
    "Variable curve AD, ASR or looping AD",
    Color::Yellow,
    AppIcon::AdEnv,
)
.add_param(Param::MidiIn)
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::bool {
    name: "MIDI retrigger",
});

#[derive(Default)]
pub struct Params {
    midi_in: MidiIn,
    midi_channel: MidiChannel,
    retrigger: bool,
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            midi_in: MidiIn::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            retrigger: bool::from_value(values[2]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.midi_in.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.retrigger.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; 2],
    curve_saved: [Curve; 2],
    mode_saved: u8,
    att_saved: u16,
    min_gate_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2000; 2],
            curve_saved: [Curve::Linear; 2],
            mode_saved: 0,
            att_saved: 4095,
            min_gate_saved: 1,
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
    let (midi_in, midi_chan, retrigger) =
        params.query(|p| (p.midi_in, p.midi_channel, p.retrigger));
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let times_glob = app.make_global([0.0682, 0.0682]);
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    let gate_on_glob = app.make_global(0);

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let minispeed = 10.0;

    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;

    let color = [Color::Yellow, Color::Cyan, Color::Pink];

    let (curve_setting, stored_faders) = storage.query(|s| (s.curve_saved, s.fader_saved));

    leds.set(
        0,
        Led::Button,
        color[curve_setting[0] as usize],
        Brightness::Mid,
    );
    leds.set(
        1,
        Led::Button,
        color[curve_setting[1] as usize],
        Brightness::Mid,
    );

    let mut times: [f32; 2] = [0.0682, 0.0682];
    for n in 0..2 {
        times[n] = Curve::Exponential.at(stored_faders[n]) as f32 + minispeed;
    }
    times_glob.set(times);

    let mut outval = 0;
    let mut old_gate = false;
    let mut button_old = false;
    let mut timer: u32 = 5000;
    let mut start_time = 0;
    let mut trigger_to_gate = false;

    let main_loop = async {
        loop {
            app.delay_millis(1).await;
            timer += 1;
            let latch_active_layer =
                glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            let mode = storage.query(|s| s.mode_saved);
            let times = times_glob.get();
            let curve_setting = storage.query(|s| s.curve_saved);

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                // catching rising edge
                gate_on_glob.modify(|note_num| *note_num + 1);
            }
            if inputval <= 406 && oldinputval > 406 {
                gate_on_glob.modify(|note_num| (*note_num - 1).max(0));
            }
            oldinputval = inputval;

            if gate_on_glob.get() > 0 && !old_gate {
                env_state = 1;
                old_gate = true;
                start_time = timer;
            }

            if timer == start_time {
                gate_on_glob.set(gate_on_glob.get() + 1);

                trigger_to_gate = true;
            }

            if gate_on_glob.get() == 0 && old_gate {
                if mode == 1 {
                    env_state = 2;
                }
                old_gate = false;
            }
            if timer - start_time > storage.query(|s: &Storage| s.min_gate_saved) as u32 + 10
                && trigger_to_gate
                && storage.query(|s: &Storage| s.min_gate_saved) != 4095
            {
                gate_on_glob.modify(|note_num| (*note_num - 1).max(0));

                trigger_to_gate = false;
            }

            if env_state == 1 {
                if times[0] == minispeed {
                    vals = 4095.0;
                }

                vals += 4095.0 / times[0];
                if vals > 4094.0 {
                    if mode != 1 {
                        env_state = 2;
                    }
                    vals = 4094.0;
                    if mode == 0 && retrigger {
                        gate_on_glob.set(0);
                    }
                }
                outval = curve_setting[0].at(vals as u16);

                leds.set(
                    0,
                    Led::Top,
                    Color::White,
                    Brightness::Custom((outval / 16) as u8),
                );
                leds.unset(1, Led::Top);
            }

            if env_state == 2 {
                vals -= 4095.0 / times[1];
                leds.unset(0, Led::Top);
                if vals < 0.0 {
                    env_state = 0;
                    vals = 0.0;
                }
                outval = curve_setting[1].at(vals as u16);

                leds.set(
                    1,
                    Led::Top,
                    Color::White,
                    Brightness::Custom((outval / 16) as u8),
                );

                if vals == 0.0 && mode == 2 && gate_on_glob.get() != 0 {
                    env_state = 1;
                }
            }
            outval = attenuate(outval, storage.query(|s| s.att_saved));
            output.set_value(outval);
            if latch_active_layer == LatchLayer::Alt {
                leds.set(1, Led::Button, color[mode as usize], Brightness::Mid);

                let att = storage.query(|s| s.att_saved);
                leds.set(
                    1,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((att / 16) as u8),
                );
                if timer % (storage.query(|s: &Storage| s.min_gate_saved) as u32 + 10) + 200
                    < (storage.query(|s: &Storage| s.min_gate_saved) as u32 + 10)
                {
                    leds.set(0, Led::Top, Color::Red, Brightness::High);
                } else {
                    leds.unset(0, Led::Top);
                }
            } else {
                for n in 0..2 {
                    leds.set(
                        n,
                        Led::Button,
                        color[curve_setting[n] as usize],
                        Brightness::Mid,
                    );
                    if outval == 0 {
                        leds.unset(n, Led::Top);
                    }
                }
            }
            if gate_on_glob.get() > 0 {
                leds.set(0, Led::Bottom, Color::Red, Brightness::High);
            } else {
                leds.set(0, Led::Bottom, Color::Red, Brightness::Mid);
            }

            if button_old && !buttons.is_button_pressed(0) && buttons.is_shift_pressed() {
                gate_on_glob.modify(|note_num| (*note_num - 1).max(0));
            }

            button_old = buttons.is_button_pressed(0);
        }
    };

    let fader_handler = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
        ];

        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            if chan == 0 {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.min_gate_saved),
                    LatchLayer::Third => 0,
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            times[chan] =
                                Curve::Exponential.at(faders.get_value_at(chan)) as f32 + minispeed;
                            times_glob.set(times);

                            storage.modify_and_save(|s| {
                                s.fader_saved[chan] = new_value;
                            });
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| {
                                s.min_gate_saved = new_value;
                            });
                        }
                        LatchLayer::Third => {}
                    }
                }
            } else {
                let target_value = match latch_layer {
                    LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                    LatchLayer::Alt => storage.query(|s| s.att_saved),
                    _ => unreachable!(),
                };
                if let Some(new_value) =
                    latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                {
                    match latch_layer {
                        LatchLayer::Main => {
                            times[chan] =
                                Curve::Exponential.at(faders.get_value_at(chan)) as f32 + minispeed;
                            times_glob.set(times);

                            storage.modify_and_save(|s| {
                                s.fader_saved[chan] = new_value;
                            });
                        }
                        LatchLayer::Alt => {
                            storage.modify_and_save(|s| {
                                s.att_saved = new_value;
                            });
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    };

    let button_handler = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if !is_shift_pressed {
                let mut curve_setting = storage.query(|s| s.curve_saved);

                curve_setting[chan] = curve_setting[chan].cycle();

                storage.modify_and_save(|s| {
                    s.curve_saved = curve_setting;
                    s.curve_saved
                });
            } else if chan == 1 {
                let mut mode = storage.query(|s| s.mode_saved);
                mode = (mode + 1) % 3;

                storage.modify_and_save(|s| {
                    s.mode_saved = mode;
                    s.mode_saved
                });
            } else if chan == 0 {
                gate_on_glob.modify(|note_num| *note_num + 1);
                // info!("here 2, gate count = {}", gate_on_glob.get().await)
            }
        }
    };

    let midi_handler = async {
        let mut midi_in = app.use_midi_input(midi_in, midi_chan);
        loop {
            match midi_in.wait_for_message().await {
                MidiMessage::NoteOn { key: _, vel } => {
                    if vel > 0 {
                        gate_on_glob.modify(|note_num| *note_num + 1);
                    } else {
                        gate_on_glob.modify(|note_num| (*note_num - 1).max(0));
                    }
                }
                MidiMessage::NoteOff { .. } => {
                    gate_on_glob.modify(|note_num| (*note_num - 1).max(0));
                }
                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;

                    let curve_setting = storage.query(|s| s.curve_saved);
                    let stored_faders = storage.query(|s| s.fader_saved);

                    leds.set(
                        0,
                        Led::Button,
                        color[curve_setting[0] as usize],
                        Brightness::Mid,
                    );
                    leds.set(
                        1,
                        Led::Button,
                        color[curve_setting[1] as usize],
                        Brightness::Mid,
                    );

                    let mut times: [f32; 2] = [0.0682, 0.0682];
                    for n in 0..2 {
                        times[n] = Curve::Exponential.at(stored_faders[n]) as f32 + minispeed;
                    }
                    times_glob.set(times);
                }
                SceneEvent::SaveScene(scene) => storage.save_to_scene(scene).await,
            }
        }
    };

    if midi_in.is_none() {
        join4(main_loop, fader_handler, button_handler, scene_handler).await;
    } else {
        join5(
            main_loop,
            fader_handler,
            button_handler,
            midi_handler,
            scene_handler,
        )
        .await;
    }
}
