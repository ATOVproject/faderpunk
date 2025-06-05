use embassy_futures::{
    join::{join, join3, join4, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::app::{App, Arr, Global, Led, Range, SceneEvent};
use config::{Config, Curve, Param};
use libfp::constants::{CURVE_EXP, CURVE_LOG};

use super::temp_param_loop;

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("AD Envelope", "FIXME");

#[derive(Serialize, Deserialize, Default)]
pub struct Storage {
    fader_saved: Arr<u16, 2>,
    curve_saved: Arr<u8, 2>,
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    select(join(run(&app), temp_param_loop()), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();

    let times_glob = app.make_global([0.0682, 0.0682]);
    let glob_curve = app.make_global([0, 0]);

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let minispeed = 10.0;
    let fadstep = 1;

    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;

    let latched_glob = app.make_global([false; 2]);

    let storage: Mutex<NoopRawMutex, Storage> =
    Mutex::new(app.load(None).await.unwrap_or(Storage::default()));

    // FIXME: Definitely improve this API
    // let stor = storage.lock().await;
    // let muted = stor.fader_saved;
    // drop(stor);

    let color = [(243, 191, 78), (188, 77, 216), (78, 243, 243)];
    let stor = storage.lock().await;
    let curve_setting = stor.curve_saved;
    drop(stor);


    leds.set(0, Led::Button, color[curve_setting.0[0] as usize], 100);
    leds.set(1, Led::Button, color[curve_setting.0[1] as usize], 100);

    let fut1 = async {
        loop {
            let color = (255, 255, 255);

            app.delay_millis(1).await;
            let times = times_glob.get().await;

            let stor = storage.lock().await;
            let curve_setting = stor.curve_saved;
            drop(stor);
            //let curve_setting = glob_curve.get().await;

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 {
                //detect passing the threshold
                env_state = 1;
                oldinputval = inputval;
            } else {
                oldinputval = inputval;
            }

            if env_state == 1 {
                if times[0] == minispeed {
                    vals = 4095.0;
                }

                vals = vals + (4095.0 / times[0]);
                if vals > 4094.0 {
                    env_state = 2;
                    vals = 4094.0;
                }
                let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];

                output.set_value_with_curve(curve[curve_setting.0[0]as usize], vals as u16);
                leds.set(0, Led::Bottom, color, (255.0 - (vals as f32) / 32.0) as u8);
                leds.set(0, Led::Top, color, (vals as f32 / 32.0) as u8);
                if vals == 4094.0 {
                    leds.set(0, Led::Top, (0, 0, 0), 0);
                    leds.set(0, Led::Bottom, (0, 0, 0), 0);
                }
                leds.set(1, Led::Top, (0, 0, 0), 0);
                leds.set(1, Led::Bottom, (0, 0, 0), 0);
            }

            if env_state == 2 {
                vals = vals - (4095.0 / times[1]);
                if vals < 0.0 {
                    env_state = 0;
                    vals = 0.0;
                }
                let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];
                output.set_value_with_curve(curve[curve_setting.0[1] as usize], vals as u16);
                leds.set(1, Led::Top, color, (vals as f32 / 32.0) as u8);
                leds.set(1, Led::Bottom, color, (255.0 - (vals as f32) / 32.0) as u8);
                

                if vals == 0.0 {
                    leds.set(1, Led::Bottom, (0, 0, 0), 0);
                }
            }
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            

            if chan < 2 {
                let vals = faders.get_values();
                let mut times = times_glob.get().await;

                let mut stor = storage.lock().await;
                let mut stored_faders = stor.fader_saved;
                let mut latched =latched_glob.get().await;

                if return_if_close(vals[chan], stored_faders.0[chan]) {
                latched[chan] = true;
                latched_glob.set(latched).await;
                }
                
                if latched[chan] {
                    stored_faders.0[chan] = vals[chan];
                    stor.fader_saved = stored_faders;
                    app.save(&*stor, None).await;
                
                    times[chan] = CURVE_LOG[vals[chan] as usize] as f32 + minispeed;
                    // (4096.0 - CURVE_EXP[vals[chan] as usize] as f32) * fadstep + minispeed;
                    times_glob.set(times).await;
                }
            }
            
        }
    };

    let fut3 = async {
        loop {
            let chan = buttons.wait_for_any_down().await;
            let stor = storage.lock().await;
            let mut curve_setting = stor.curve_saved;
            drop(stor);

            
            curve_setting.0[chan] = (curve_setting.0[chan] + 1) % 3;
            leds.set(chan, Led::Button, color[curve_setting.0[chan] as usize], 75);

            
            let mut stor = storage.lock().await;
            stor.curve_saved = curve_setting;
            app.save(&*stor, None).await;
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    let mut stor = storage.lock().await;
                    let scene_stor = app.load(Some(scene)).await.unwrap_or(Storage::default());
                    *stor = scene_stor;

                    //recall fader state and do the math
                    let stored_faders = stor.fader_saved;

                    let mut times: [f32; 2] = times_glob.get().await;

                    for chan in 0..=1 {
                        times[chan]= CURVE_LOG[stored_faders.0[chan] as usize] as f32 + minispeed;
                    }
                    latched_glob.set([false, false]).await;
                    times_glob.set(times).await;
                    let curve_setting = stor.curve_saved;
                    leds.set(0, Led::Button, color[curve_setting.0[0] as usize], 100);
                    leds.set(1, Led::Button, color[curve_setting.0[1] as usize], 100);
                    
                }
                SceneEvent::SaveScene(scene) => {
                    let stor = storage.lock().await;
                    app.save(&*stor, Some(scene)).await;
                }
            }
        }
    };

    join4(fut1, fut2, fut3, scene_handler).await;
}


fn return_if_close(a: u16, b: u16) -> bool {
    if a.abs_diff(b) < 100 {
        true
    } else {
        false
    }
}