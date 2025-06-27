// TODO : 
// add saving to modes

use config::{Config, Curve};
use defmt::info;
use embassy_futures::{
    join::{join, join4, join5},
    select::select,
};
use embassy_sync::{blocking_mutex::{raw::NoopRawMutex, Mutex}, signal::Signal};
use libfp::constants::CURVE_LOG;
use serde::{Deserialize, Serialize};

use crate::app::{
    App, AppStorage, Led, ManagedStorage, ParamStore, Range, SceneEvent,
};

pub const CHANNELS: usize = 2;
pub const PARAMS: usize = 0;

pub static CONFIG: config::Config<PARAMS> = Config::new("AD Envelope", "variable curve AD, ASR or looping AD");

#[derive(Serialize, Deserialize)]


pub struct Storage {
    fader_saved: [u16; 2],
    curve_saved: [u8; 2],
    mode_saved: u8,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2000; 2],
            curve_saved: [0; 2],
            mode_saved: 0,
        }
    }
}

impl AppStorage for Storage {}

pub struct Params {}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::new([], app.app_id, app.start_channel);
    let params = Params {};

    let app_loop = async {
        loop {
            let storage = ManagedStorage::<Storage>::new(app.app_id, app.start_channel);
            select(run(&app, &params, storage), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, _params: &Params, storage: ManagedStorage<Storage>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    //let midi = app.use_midi(0);

    let times_glob = app.make_global([0.0682, 0.0682]);
    let glob_curve = app.make_global([0, 0]);
    let mode_glob = app.make_global(1); //0 AD, 1, ASR, 2 Looping AD

    let input = app.make_in_jack(0, Range::_0_10V).await;
    let output = app.make_out_jack(1, Range::_0_10V).await;

    let minispeed = 10.0;

    let mut vals: f32 = 0.0;
    let mut oldinputval = 0;
    let mut env_state = 0;

    let latched_glob = app.make_global([false; 2]);

    let colorb = [(243, 191, 78), (188, 77, 216), (78, 243, 243)];


    // storage.load(None).await;

    // let curve_setting = storage.query(|s| s.curve_saved).await;
    // let stored_faders = storage.query(|s| s.fader_saved).await;
    // glob_curve.set(curve_setting).await;


    // leds.set(0, Led::Button, color[curve_setting[0] as usize], 100);
    // leds.set(1, Led::Button, color[curve_setting[1] as usize], 100);

    // let mut times: [f32; 2] = [0.0682, 0.0682];
    // for n in 0..1{
    //     times[n]= CURVE_LOG[stored_faders[n] as usize] as f32 + minispeed;
    // }
    // times_glob.set(times).await;


    let fut1 = async {
        loop {
            let color = (255, 255, 255);

            app.delay_millis(1).await;
            let mode = mode_glob.get().await;
            let times = times_glob.get().await;

            let curve_setting = glob_curve.get().await;

            let inputval = input.get_value();
            if inputval >= 406 && oldinputval < 406 { // catching rising edge
                env_state = 1;  
            } 
            if mode == 1 && inputval <= 406 && oldinputval > 406 {
                env_state = 2;
            }
            oldinputval = inputval;


            if env_state == 1 {
                if times[0] == minispeed {
                    vals = 4095.0;
                }

                vals = vals + (4095.0 / times[0]);
                if vals > 4094.0 {
                    if mode != 1{
                        env_state = 2;
                    }
                    vals = 4094.0;
                }
                let curve: [Curve; 3] = [Curve::Linear, Curve::Exponential, Curve::Logarithmic];

                output.set_value_with_curve(curve[curve_setting[0]as usize], vals as u16);
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
                output.set_value_with_curve(curve[curve_setting[1] as usize], vals as u16);
                leds.set(1, Led::Top, color, (vals as f32 / 32.0) as u8);
                leds.set(1, Led::Bottom, color, (255.0 - (vals as f32) / 32.0) as u8);
                

                if vals == 0.0 {
                    leds.set(1, Led::Bottom, (0, 0, 0), 0);
                    if mode == 2 && inputval > 406 {
                        env_state = 1;
                    }
                }
            }

            if buttons.is_shift_pressed(){
                leds.set(0, Led::Button, colorb[mode as usize], 75);
                leds.set(1, Led::Button, colorb[0], 0);
            }
            if !buttons.is_shift_pressed(){
                for n in 0..2 {
                    leds.set(n, Led::Button, colorb[curve_setting[n] as usize], 75);
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


                let mut stored_faders = storage.query(|s| s.fader_saved).await;

                let mut latched = latched_glob.get().await;

                if return_if_close(vals[chan], stored_faders[chan]) {
                latched[chan] = true;
                latched_glob.set(latched).await;
                }
                
                if latched[chan] {
                    stored_faders[chan] = vals[chan];
                    // stor.fader_saved = stored_faders;
                    // app.save(&*stor, None).await;

                    storage.modify_and_save(|s| {
                            s.fader_saved = stored_faders;
                            s.fader_saved
                        },
                        None
                    ).await;
                
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
            if !buttons.is_shift_pressed() {
                let mut curve_setting = storage.query(|s| s.curve_saved).await;

                
                curve_setting[chan] = (curve_setting[chan] + 1) % 3;
                glob_curve.set(curve_setting).await;
                

                
                storage.modify_and_save(|s| {
                        s.curve_saved = curve_setting;
                        s.curve_saved
                    },
                    None
                ).await;
            }

            if buttons.is_shift_pressed() && chan == 0 {
                let mut mode = mode_glob.get().await;
                mode = (mode + 1) % 3;
                mode_glob.set(mode).await;

                storage.modify_and_save(|s| {
                        s.mode_saved = mode;
                        s.mode_saved
                    },
                    None
                ).await;
            }
        }

    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;



                    let curve_setting = storage.query(|s| s.curve_saved).await;
                    let stored_faders = storage.query(|s| s.fader_saved).await;
                    let mode = storage.query(|s| s.mode_saved).await;
                    glob_curve.set(curve_setting).await;
                    mode_glob.set(mode).await;
                
                
                    leds.set(0, Led::Button, colorb[curve_setting[0] as usize], 100);
                    leds.set(1, Led::Button, colorb[curve_setting[1] as usize], 100);
                
                    let mut times: [f32; 2] = [0.0682, 0.0682];
                    for n in 0..1{
                        times[n]= CURVE_LOG[stored_faders[n] as usize] as f32 + minispeed;
                    }
                    times_glob.set(times).await;


                    
                }
                SceneEvent::SaveScene(scene) => storage.save(Some(scene)).await,
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