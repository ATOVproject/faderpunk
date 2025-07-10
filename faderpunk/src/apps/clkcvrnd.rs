// Todo :
// Save div, mute, attenuation - Added the saving slots, need to add write/read in the app.
// Add attenuator (shift + fader)

use config::{Config, Param, Value};
use embassy_futures::{join::{join5}, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use serde::{Deserialize, Serialize};
use smart_leds::{RGB, RGB8};

use crate::app::{App, AppStorage, ClockEvent, Led, ManagedStorage, ParamSlot, ParamStore, Range, SceneEvent};

use libfp::utils::{attenuate, attenuate_bipolar, is_close, scale_bits_12_7, split_unsigned_value};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 2;

pub static CONFIG: config::Config<PARAMS> = Config::new("Random CC/CV", "Generate random values on clock")
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



pub struct Params<'a> {
    midi_channel: ParamSlot<'a, i32, PARAMS>,
    cc: ParamSlot<'a, i32, PARAMS>,
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: u16,
    mute_save: bool,
    att_saved: u16
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: 3000,
            mute_save: false,
            att_saved: 4096,

        }
    }
}
impl AppStorage for Storage {}


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
    let mut clock = app.use_clock();
    let mut rnd = app.use_die();
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let midi_chan = params.midi_channel.get().await;
    let cc = params.cc.get().await;
    let midi = app.use_midi(midi_chan as u8 - 1);

    let glob_muted = app.make_global(false);
    let div_glob = app.make_global(6);
    let att_glob = app.make_global(4096);
    let latched_glob = app.make_global(false);

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

   

    let mut clkn = 0;
    let mut val = 2048;

    const LED_COLOR: RGB<u8> = RGB8 {
        r: 243,
        g: 191,
        b: 78,};

    storage.load(None).await;

    let (res, mute, att) =
        storage
            .query(|s| (s.fader_saved, s.mute_save, s.att_saved))
            .await;

    att_glob.set(att).await;
    glob_muted.set(mute).await;
    div_glob.set(resolution[res as usize / 345]).await;
    if mute {
        leds.set(0, Led::Button, LED_COLOR, 0);
        output.set_value(2047);
        midi.send_cc(cc as u8, 0).await;
        leds.set(0, Led::Top, LED_COLOR, 0);
        leds.set(0, Led::Bottom, LED_COLOR, 0);
    } else {
        leds.set(0, Led::Button, LED_COLOR, 75);
    }  


    let fut1 = async {
        loop {
            
            

            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    clkn += 1;
                    let muted = glob_muted.get().await;
                    let att = att_glob.get().await;
                    let div = div_glob.get().await;
                    if clkn % div == 0 && !muted {
                        let midival = attenuate(val, att);
                        let jackval = attenuate_bipolar(val, att);
                        output.set_value(jackval);
                        midi.send_cc(cc as u8, midival).await;
                        let ledj = split_unsigned_value(jackval);
                        leds.set(0, Led::Top, LED_COLOR, ledj[0]);
                        leds.set(0, Led::Bottom, LED_COLOR, ledj[1]);
                        val = rnd.roll();
                    }
                }
                _ => {}
            }
            

        
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_any_down().await;
            let muted = glob_muted.toggle().await;
            
            storage
            .modify_and_save(
                |s| {
                    s.mute_save = muted;
                    s.mute_save
                },
                None,
            )
            .await;
            
            if muted {
                leds.set(0, Led::Button, LED_COLOR, 0);
                output.set_value(2047);
                midi.send_cc(cc as u8, 0).await;
                leds.set(0, Led::Top, LED_COLOR, 0);
                leds.set(0, Led::Bottom, LED_COLOR, 0);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 75);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change_at(0).await;
            storage.load(None).await;
            let fad = fader.get_value();
            
            
            if !buttons.is_shift_pressed() {
                let fad_saved = storage.query(|s| s.fader_saved).await;
                if is_close(fad, fad_saved){
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await {
                    div_glob.set(resolution[fad as usize / 345]).await;
                    storage.modify_and_save(|s| s.fader_saved = fad, None).await;

                }
                
                
            }
            else {
                let att = att_glob.get().await;
                if is_close(fad, att){
                    latched_glob.set(true).await;
                }
                if latched_glob.get().await{
                    att_glob.set(fad).await;
                    storage.modify_and_save(|s| s.att_saved = fad, None).await;
                }
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    storage.load(Some(scene)).await;
                    let (res, mute, att) =
                        storage
                            .query(|s| (s.fader_saved, s.mute_save, s.att_saved))
                            .await;

                    att_glob.set(att).await;
                    glob_muted.set(mute).await;
                    div_glob.set(resolution[res as usize / 345]).await;
                    if mute {
                        leds.set(0, Led::Button, LED_COLOR, 0);
                        output.set_value(2047);
                        midi.send_cc(cc as u8, 0).await;
                        leds.set(0, Led::Top, LED_COLOR, 0);
                        leds.set(0, Led::Bottom, LED_COLOR, 0);
                    } else {
                        leds.set(0, Led::Button, LED_COLOR, 75);
                    }                   
                }



                SceneEvent::SaveScene(scene) => {
                    storage.save(Some(scene)).await;

                
                }
            }
        }
    };

    let mut shift_old = false;

    let shift = async {
        loop {
            // latching on pressing and depressing shift

            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = true;
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set(false).await;
                shift_old = false;
            }
        }
    };

    

    join5(fut1, fut2, fut3, scene_handler, shift).await;
}

