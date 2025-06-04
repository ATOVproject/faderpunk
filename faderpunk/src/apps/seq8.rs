//To Do:
//add quantizer
//add per channel gate seq_length
//add MIDI param

use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::{
    join::{join, join5, join_array},
    select::select,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, signal::Signal};
use serde::{Deserialize, Serialize};
use smart_leds::brightness;

use crate::app::{App, Arr, Led, Range, SceneEvent};

use super::temp_param_loop;

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 0;

pub static CONFIG: Config<PARAMS> = Config::new("Sequencer", "4 x 16 step CV/gate sequencers");

#[derive(Serialize, Deserialize)]
pub struct Storage {
    seq_glob: Arr<u16, 64>,
    gateseq_glob: Arr<bool, 64>,
    seq_length_glob: Arr<u8, 4>,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            seq_glob: Arr([0; 64]),
            gateseq_glob: Arr([true; 64]),
            seq_length_glob: Arr([16; 4]),
        }
    }
}

#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    // TODO: Do PARAM loop here
    // TODO: We _could_ do some storage stuff in here.
    // FIXME: It COULD be that the signal.wait() immediately resolves for some reason
    select(join(run(&app), temp_param_loop()), exit_signal.wait()).await;
}

pub async fn run(app: &App<CHANNELS>) {
    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let mut clk = app.use_clock();
    let led = app.use_leds();

    let midi = [
        app.use_midi(0),
        app.use_midi(1),
        app.use_midi(2),
        app.use_midi(3),
    ];

    let clockn_glob = app.make_global(0);
    let gatet = 20;

    let cv_out = [
        app.make_out_jack(0, Range::_0_10V).await,
        app.make_out_jack(2, Range::_0_10V).await,
        app.make_out_jack(4, Range::_0_10V).await,
        app.make_out_jack(6, Range::_0_10V).await,
    ];
    //let cv1 = app.make_out_jack(2, Range::_0_10V).await;
    let gate_out = [
        app.make_gate_jack(1, 4095).await,
        app.make_gate_jack(3, 4095).await,
        app.make_gate_jack(5, 4095).await,
        app.make_gate_jack(7, 4095).await,
    ];

    // let mut seq_glob = app.make_global_with_store(Arr([0; 64]), StorageSlot::A);
    // seq_glob.load().await;
    // let mut gateseq_glob = app.make_global_with_store(Arr([true; 64]), StorageSlot::B);
    // gateseq_glob.load().await;
    // let mut seq_length_glob = app.make_global_with_store(Arr([16; 4]), StorageSlot::C);
    // seq_length_glob.load().await;

    let storage: Mutex<NoopRawMutex, Storage> =
        Mutex::new(app.load(None).await.unwrap_or(Storage::default()));

    //recall from memory
    let stor = storage.lock().await;
    // let seq_glob = stor.seq_glob;
    // let gateseq_glob = stor.gateseq_glob;
    // let seq_length_glob = stor.seq_length_glob;
    // stor.seq_length_glob = Arr([16; 4]);
    // app.save(&*stor, None).await;

    drop(stor);

    //let mut latched_glob = app.make_global([true, true, true, true, true, true, true, true]);

    let page_glob = app.make_global(0);
    let led_flag_glob = app.make_global(true);
    let lenght_flag = app.make_global(false);
    let latched_glob = app.make_global([false; 8]);

    // let div = app.make_global(1);
    // let shif_old = false;
    // let gate_flag_glob = app.make_global([false, false, false, false]);
    let mut shift_old = false;

    let fut1 = async {
        loop {
            // latching on pressing and depressing shift
            app.delay_millis(1).await;
            if !shift_old && buttons.is_shift_pressed() {
                latched_glob.set([false; 8]).await;
                shift_old = true;
                info!("unlatch everything")
            }
            if shift_old && !buttons.is_shift_pressed() {
                latched_glob.set([false; 8]).await;
                shift_old = false;
                info!("unlatch everything again")
            }
        }
    };

    let fut2 = async {
        //Fader handling - Should be latching false when shift is pressed
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let page = page_glob.get().await;

            let stor = storage.lock().await;
            let mut seq = stor.seq_glob;
            let mut seq_length = stor.seq_length_glob;
            drop(stor);

            // let mut seq_length = seq_length_glob.get_array().await;
            // let mut seq = seq_glob.get_array().await;

            let _shift = buttons.is_shift_pressed();
            let mut latched = latched_glob.get().await;

            if return_if_close(vals[chan], seq.0[chan + (page * 8)]) && !_shift {
                latched[chan] = true;
                latched_glob.set(latched).await;
            }

            if !_shift && chan < 8 && latched[chan] && latched[chan] {
                seq.0[chan + (page * 8)] = vals[chan];
                let mut stor = storage.lock().await;
                stor.seq_glob = seq;
                app.save(&*stor, None).await;
            }

            if (vals[0] / 256 + 1) as u8 == seq_length.0[page / 2] && _shift {
                latched[0] = true;
                latched_glob.set(latched).await;
                //info!("latching!");
            }

            if _shift {
                // add check for latching
                if chan == 0 && latched[0] {
                    //fader 1 + shift
                    seq_length.0[(page / 2)] = (((vals[0]) / 256) + 1) as u8;
                    //info!("{}", seq_length[page / 2]);
                    let mut stor = storage.lock().await;
                    stor.seq_length_glob = seq_length;
                    app.save(&*stor, None).await;

                    // seq_length_glob.set_array(seq_length).await;
                    // seq_length_glob.save().await;
                    lenght_flag.set(true).await;
                }

                if chan % 2 == 1 {}
            }
            led_flag_glob.set(true).await;
        }
    };

    let fut3 = async {
        //button handling
        //Short button presses
        loop {
            let chan = buttons.wait_for_any_down().await;

            let stor = storage.lock().await;
            // let seq = stor.seq_glob;
            let mut gateseq = stor.gateseq_glob;
            // let seq_length = stor.seq_length_glob;
            drop(stor);

            // let mut gateseq = gateseq_glob.get_array().await;
            let _shift = buttons.is_shift_pressed();
            let page = page_glob.get().await;
            if !_shift {
                gateseq.0[chan + (page * 8)] = !gateseq.0[chan + (page * 8)];

                let mut stor = storage.lock().await;
                stor.gateseq_glob = gateseq;
                app.save(&*stor, None).await;
                drop(stor);

                // gateseq_glob.set_array(gateseq).await;
                // gateseq_glob.save().await;
                led_flag_glob.set(true).await;
            }

            if _shift {
                page_glob.set(chan).await;
                led_flag_glob.set(true).await;
                latched_glob.set([false; 8]).await
            }
        }
    };

    let fut4 = async {
        //LED update
        loop {
            let intencity = [50, 100, 200];
            let colours = [
                (243, 191, 78),
                (188, 77, 216),
                (78, 243, 243),
                (250, 250, 250),
            ];
            app.delay_millis(10).await;

            //if buttons.is_shift_pressed().await;
            if buttons.is_shift_pressed() {
                let clockn = clockn_glob.get().await;

                //let seq_length = seq_length_glob.get_array().await;

                let stor = storage.lock().await;
                let seq_length = stor.seq_length_glob;
                drop(stor);

                let page = page_glob.get().await;
                let mut bright = 75;
                for n in 0..=7 {
                    if n == page {
                        bright = intencity[2];
                    } else {
                        bright = intencity[1];
                    }
                    led.set(n, Led::Button, colours[n / 2], bright);
                }
                for n in 0..=15 {
                    if n < seq_length.0[page / 2] {
                        bright = 100
                    }
                    if n == clockn as u8 % seq_length.0[page / 2] {
                        bright = 200
                    }
                    if n >= seq_length.0[page / 2] {
                        bright = 0
                    }
                    if n < 8 {
                        led.set(n as usize, Led::Top, (255, 0, 0), bright)
                    } else {
                        led.set(n as usize - 8, Led::Bottom, (255, 0, 0), bright)
                    }
                }
            }

            // let led_flag = led_flag_glob.get().await;
            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get().await;

                let stor = storage.lock().await;
                let seq = stor.seq_glob;
                let gateseq = stor.gateseq_glob;
                let seq_length = stor.seq_length_glob;
                drop(stor);

                // let gateseq = gateseq_glob.get_array().await;
                // let seq_length = seq_length_glob.get_array().await; //use this to highlight active notes
                // let seq = seq_glob.get_array().await;

                let mut colour = (243, 191, 78);
                let clockn = clockn_glob.get().await;

                if page / 2 == 0 {
                    colour = (243, 191, 78);
                }
                if page / 2 == 1 {
                    colour = (188, 77, 216);
                }
                if page / 2 == 2 {
                    colour = (78, 243, 243);
                }
                if page / 2 == 3 {
                    colour = (250, 250, 250);
                }

                for n in 0..=7 {
                    led.set(n, Led::Top, colour, (seq.0[n + (page * 8)] / 16) as u8 / 2);

                    if gateseq.0[n + (page * 8)] {
                        led.set(n, Led::Button, colour, intencity[1]);

                        //led.set(n, Led::Bottom , colour, 0);
                    }
                    if !gateseq.0[n + (page * 8)] {
                        led.set(n, Led::Button, colour, intencity[0]);
                        //led.set(n, Led::Bottom , colour, 0);
                    }

                    let index = seq_length.0[page / 2] as usize - (page % 2 * 8);
                    //info!("{}", index);

                    if n >= index || index > 16 {
                        led.set(n, Led::Button, colour, 0);
                    }

                    if (clockn % seq_length.0[n / 2] as usize) % 16 - (n % 2) * 8 < 8 {
                        led.set(n, Led::Bottom, (255, 0, 0), 100)
                    } else {
                        led.set(n, Led::Bottom, (255, 0, 0), 0)
                    }
                }
                //runing light on buttons
                if (clockn % seq_length.0[page / 2] as usize) % 16 - (page % 2) * 8 < 8 {
                    led.set(
                        (clockn % seq_length.0[page / 2] as usize) % 16 - (page % 2) * 8,
                        Led::Button,
                        (255, 0, 0),
                        100,
                    );
                }

                led.set(page, Led::Bottom, colour, 255);
            }

            led_flag_glob.set(false).await;
        }
    };

    let fut5 = async {
        //sequencer functions
        loop {
            let stor = storage.lock().await;
            // let seq = stor.seq_glob;
            let gateseq = stor.gateseq_glob;
            let seq_length = stor.seq_length_glob;
            drop(stor);

            // let gateseq = gateseq_glob.get_array().await;
            // let seq_length = seq_length_glob.get_array().await;

            let mut clockn = clockn_glob.get().await;
            // let page = page_glob.get().await;
            let reset = clk.wait_for_tick(6).await;
            clockn += 1;
            if reset {
                clockn = 0;
                clockn_glob.set(clockn).await;
            }
            if !reset {
                clockn_glob.set(clockn).await;
                //led.set((clockn % seq_length[page / 2] as usize) % 8, Led::Button, (255, 0, 0), 100 );
                let stor = storage.lock().await;
                let seq = stor.seq_glob;
                drop(stor);

                for n in 0..=3 {
                    let clkindex = ((clockn % seq_length.0[n] as usize) + (n * 16));

                    if gateseq.0[clkindex] {
                        gate_out[n].set_high().await;
                        cv_out[n].set_value(seq.0[clkindex] / 4); //only update CV out on active step
                        midi[n]
                            .send_note_on((seq.0[clkindex] / 170) as u8 + 60, 4095)
                            .await;

                        //gate_flag_glob[n].set(true).await;

                        //app.delay_millis(gatet).await;
                        //gate_out[n].set_low().await;
                    } else {
                        // note that are not triggered
                        gate_out[n].set_low().await;

                        //midi[n].send_note_off((seq[clkindex] / 170) as u8 + 60).await
                    }
                    //led_flag_glob.set(true).await;
                }

                app.delay_millis(gatet).await;
                for n in 0..=3 {
                    let clkindex = (clockn % seq_length.0[n] as usize) + (n * 16);
                    if gateseq.0[clkindex] {
                        //gate_out[n].set_high().await;
                        //gate_flag_glob[n].set(true).await;

                        //app.delay_millis(gatet).await;
                        gate_out[n].set_low().await;

                        midi[n]
                            .send_note_off((seq.0[clkindex] / 170) as u8 + 60)
                            .await
                    }

                    led_flag_glob.set(true).await;
                }
                //clockn += 1;
            }
        }
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadSscene(scene) => {
                    defmt::info!("LOADING SCENE {}", scene);
                    let mut stor = storage.lock().await;
                    let scene_stor = app.load(Some(scene)).await.unwrap_or(Storage::default());
                    *stor = scene_stor;
                    //update_outputs(stor.muted).await;
                }
                SceneEvent::SaveScene(scene) => {
                    defmt::info!("SAVING SCENE {}", scene);
                    let stor = storage.lock().await;
                    app.save(&*stor, Some(scene)).await;
                }
            }
        }
    };

    join(join5(fut1, fut2, fut3, fut4, fut5), scene_handler).await;
}

fn return_if_close(a: u16, b: u16) -> bool {
    a.abs_diff(b) < 100
}
