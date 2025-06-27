// Todo :
// Save div, mute, attenuation - Added the saving slots, need to add write/read in the app.
// Add attenuator (shift + fader)

use config::{Config, Param, Value};
use defmt::info;
use embassy_futures::{join::{join3, join4, join5}, select::select};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use serde::{Deserialize, Serialize};

use crate::{app::{App, Arr, ClockEvent, Led, Range, SceneEvent}, storage::{ParamSlot, ParamStore}};

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
            select(run(&app, &params), param_store.param_handler()).await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

pub async fn run(app: &App<CHANNELS>, params: &Params<'_>) {
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

    let output = app.make_out_jack(0, Range::_Neg5_5V).await;

    let resolution = [368, 184, 92, 48, 24, 16, 12, 8, 6, 4, 3, 2];

   

    let mut clkn = 0;
    let mut val = 2048;

    const LED_COLOR: (u8, u8, u8) = (188, 77, 216);

    leds.set(0, Led::Button, LED_COLOR, 100);

    let fut1 = async {
        loop {

            

            match clock.wait_for_event(1).await {
                ClockEvent::Reset => {
                    clkn = 0;
                }
                ClockEvent::Tick => {
                    clkn += 1;
                }
                _ => {}
            }
            
            let muted = glob_muted.get().await;
            
            let div = div_glob.get().await;
            if clkn % div == 0 && !muted {
                output.set_value(val);
                midi.send_cc(36, val).await;
                leds.set(0, Led::Top, LED_COLOR, (val / 16) as u8);
                leds.set(0, Led::Bottom, LED_COLOR, (255 - val / 16) as u8);
                val = rnd.roll();
            }
        
        }
    };

    let fut2 = async {
        loop {
            buttons.wait_for_any_down().await;
            let muted = glob_muted.toggle().await;
            if muted {
                leds.set(0, Led::Button, LED_COLOR, 0);
                output.set_value(2047);
                midi.send_cc(36, 0).await;
                leds.set(0, Led::Top, LED_COLOR, 0);
                leds.set(0, Led::Bottom, LED_COLOR, 0);
            } else {
                leds.set(0, Led::Button, LED_COLOR, 75);
            }
        }
    };

    let fut3 = async {
        loop {
            fader.wait_for_change(0).await;
            let fad = fader.get_values();
            div_glob.set(resolution[fad[0] as usize / 345]).await;
        }
    };

    join3(fut1, fut2, fut3).await;
}
