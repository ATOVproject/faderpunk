// TODO :
// fix when timer is over timer th
// add trigger on midi
// Fix the saving

use defmt::info;
use embassy_futures::{join::join5, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use midly::MidiMessage;
use serde::{Deserialize, Serialize};

use libfp::{Brightness, Color, Config, Curve, Range, Value, APP_MAX_PARAMS};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> =
    Config::new("AD Envelope", "variable curve AD, ASR or looping AD");

pub struct Params {}

impl Default for Params {
    fn default() -> Self {
        Self {}
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        // if values.len() < PARAMS {
        //     return None;
        // }
        Some(Self {})
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
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
    _params: &ParamStore<Params>,
    storage: ManagedStorage<Storage>,
) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let mut midi_in = app.use_midi_input(2);

    let times_glob = app.make_global([0.0682, 0.0682]);
    let latched_glob = app.make_global([false; 2]);

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
        Brightness::Lower,
    );
    leds.set(
        1,
        Led::Button,
        color[curve_setting[1] as usize],
        Brightness::Lower,
    );

    let mut times: [f32; 2] = [0.0682, 0.0682];
    for n in 0..2 {
        times[n] = Curve::Logarithmic.at(stored_faders[n]) as f32 + minispeed;
    }
    times_glob.set(times);

    let mut outval = 0;
    let mut shift_old = false;
    let mut old_gate = false;
    let mut button_old = false;
    let mut timer: u32 = 5000;
    let mut start_time = 0;
    let mut t2g = false;

    let fut1 = async {
        loop {
            app.delay_millis(1).await;
            timer += 1;
            let mode = storage.query(|s| s.mode_saved);
            let times = times_glob.get();
            let curve_setting = storage.query(|s| s.curve_saved);

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                // catching rising edge
                gate_on_glob.modify(|g| *g + 1);
            }
            if inputval <= 406 && oldinputval > 406 {
                gate_on_glob.modify(|g| (*g - 1).max(0));
            }
            oldinputval = inputval;

            if gate_on_glob.get() > 0 && !old_gate {
                env_state = 1;
                old_gate = true;
                start_time = timer;
            }

            if timer == start_time {
                gate_on_glob.set(gate_on_glob.get() + 1);

                t2g = true;
                info!(
                    "gate on, gate count = {}, t2g = {}",
                    gate_on_glob.get(),
                    t2g
                );
            }

            if gate_on_glob.get() == 0 && old_gate {
                if mode == 1 {
                    env_state = 2;
                }
                old_gate = false;
            }
            if timer - start_time > storage.query(|s: &Storage| s.min_gate_saved) as u32
                && t2g
                && storage.query(|s: &Storage| s.min_gate_saved) != 4095
            {
                gate_on_glob.modify(|g| (*g - 1).max(0));

                t2g = false;
                info!(
                    "gate off, gate count = {}, t2g = {}",
                    gate_on_glob.get(),
                    t2g
                );
            }

            // info!(
            //     "gate on = {}, old gate = {}, env state = {}",
            //     gate_on_glob.get().await,
            //     old_gate,
            //     env_state
            // );

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
            if shift_old {
                leds.set(1, Led::Button, color[mode as usize], Brightness::Lower);
                if gate_on_glob.get() > 0 {
                    leds.set(0, Led::Button, Color::Red, Brightness::Low);
                } else {
                    leds.set(0, Led::Button, Color::Red, Brightness::Lower);
                }

                let att = storage.query(|s| s.att_saved);
                leds.set(
                    1,
                    Led::Top,
                    Color::Red,
                    Brightness::Custom((att / 16) as u8),
                );
                if timer % storage.query(|s: &Storage| s.min_gate_saved) as u32 + 200
                    < storage.query(|s: &Storage| s.min_gate_saved) as u32
                {
                    leds.set(0, Led::Top, Color::Red, Brightness::Low);
                } else {
                    leds.unset(0, Led::Top);
                }
            } else {
                for n in 0..2 {
                    leds.set(
                        n,
                        Led::Button,
                        color[curve_setting[n] as usize],
                        Brightness::Lower,
                    );
                    if outval == 0 {
                        leds.unset(n, Led::Top);
                    }
                }
            }

            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set([false; 2]);
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set([false; 2]);

                shift_old = false;
            }

            if button_old && !buttons.is_button_pressed(0) && buttons.is_shift_pressed() {
                gate_on_glob.modify(|g| (*g - 1).max(0));
            }
            // info!("{}", gate_on_glob.get().await);

            button_old = buttons.is_button_pressed(0);
        }
    };

    let fut2 = async {
        loop {
            let chan: usize = faders.wait_for_any_change().await;
            let mut latched = latched_glob.get();
            let vals = faders.get_all_values();

            if !buttons.is_shift_pressed() {
                let mut times = times_glob.get();
                let mut stored_faders = storage.query(|s| s.fader_saved);

                if is_close(vals[chan], stored_faders[chan]) {
                    latched[chan] = true;
                    latched_glob.set(latched);
                }

                if latched[chan] {
                    stored_faders[chan] = vals[chan];
                    // stor.fader_saved = stored_faders;
                    // app.save(&*stor, None).await;

                    storage
                        .modify_and_save(
                            |s| {
                                s.fader_saved = stored_faders;
                                s.fader_saved
                            },
                            None,
                        )
                        .await;

                    times[chan] = Curve::Logarithmic.at(vals[chan]) as f32 + minispeed;
                    // (4096.0 - CURVE_EXP[vals[chan] as usize] as f32) * fadstep + minispeed;
                    times_glob.set(times);
                }
            } else if chan == 1 {
                if is_close(vals[chan], storage.query(|s| s.att_saved)) {
                    latched[chan] = true;
                    latched_glob.set(latched);
                }
                if latched[chan] {
                    storage
                        .modify_and_save(
                            |s| {
                                s.att_saved = vals[chan];
                                s.att_saved
                            },
                            None,
                        )
                        .await;
                }
            } else if chan == 0 {
                if is_close(
                    vals[chan] + 10,
                    storage.query(|s: &Storage| s.min_gate_saved),
                ) {
                    latched[chan] = true;
                    latched_glob.set(latched);
                }
                if latched[chan] {
                    storage
                        .modify_and_save(
                            |s| {
                                s.min_gate_saved = vals[chan] + 10;
                                s.min_gate_saved
                            },
                            None,
                        )
                        .await;
                }
            }
        }
    };

    let fut3 = async {
        loop {
            let (chan, is_shift_pressed) = buttons.wait_for_any_down().await;
            if !is_shift_pressed {
                let mut curve_setting = storage.query(|s| s.curve_saved);

                curve_setting[chan] = curve_setting[chan].cycle();

                storage
                    .modify_and_save(
                        |s| {
                            s.curve_saved = curve_setting;
                            s.curve_saved
                        },
                        None,
                    )
                    .await;
            } else if chan == 1 {
                let mut mode = storage.query(|s| s.mode_saved);
                mode = (mode + 1) % 3;

                storage
                    .modify_and_save(
                        |s| {
                            s.mode_saved = mode;
                            s.mode_saved
                        },
                        None,
                    )
                    .await;
            } else if chan == 0 {
                gate_on_glob.modify(|g| *g + 1);
                // info!("here 2, gate count = {}", gate_on_glob.get().await)
            }
        }
    };

    let fut4 = async {
        let mut note_num = 0;
        loop {
            match midi_in.wait_for_message().await {
                MidiMessage::NoteOn { key, vel } => {
                    gate_on_glob.modify(|g| *g + 1);
                }
                MidiMessage::NoteOff { key, vel } => {
                    gate_on_glob.modify(|g| (*g - 1).max(0));
                }

                _ => {}
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;

                    let curve_setting = storage.query(|s| s.curve_saved);
                    let stored_faders = storage.query(|s| s.fader_saved);

                    leds.set(
                        0,
                        Led::Button,
                        color[curve_setting[0] as usize],
                        Brightness::Lower,
                    );
                    leds.set(
                        1,
                        Led::Button,
                        color[curve_setting[1] as usize],
                        Brightness::Lower,
                    );

                    let mut times: [f32; 2] = [0.0682, 0.0682];
                    for n in 0..2 {
                        times[n] = Curve::Logarithmic.at(stored_faders[n]) as f32 + minispeed;
                    }
                    times_glob.set(times);
                    latched_glob.set([false; 2]);
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, scene_handler).await;
}

fn is_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 75
}

fn attenuate(signal: u16, level: u16) -> u16 {
    let attenuated = (signal as u32 * level as u32) / 4095;

    attenuated as u16
}
