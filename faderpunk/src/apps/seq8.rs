//To Do:

//add led feedback for the fader position
//add quantizer
//add per channel gate seq_lenght
//add MIDI

use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::join5;

use crate::app::{App, Arr, Led, Range, StorageSlot};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Sequencer", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        default: Curve::Linear,
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    });

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;

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

    let mut seq_glob = app.make_global_with_store(Arr([0; 64]), StorageSlot::A);
    seq_glob.load().await;
    let mut gateseq_glob = app.make_global_with_store(Arr([true; 64]), StorageSlot::B);
    gateseq_glob.load().await;
    let mut seq_length_glob = app.make_global_with_store(Arr([16; 4]), StorageSlot::C);
    seq_length_glob.load().await;

    //let mut latched_glob = app.make_global([true, true, true, true, true, true, true, true]);

    let page_glob = app.make_global(0);
    let led_flag_glob = app.make_global(true);
    let div = app.make_global(1);
    let mut shif_old = false;

    let gate_flag_glob = app.make_global([false, false, false, false]);

    let fut1 = async {
        loop {
            // do the slides here
            app.delay_millis(1).await;
        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let page = page_glob.get().await;
            let mut seq = seq_glob.get_array().await;
            let _shift = buttons.is_shift_pressed();

            if !_shift && chan < 8 {
                seq[chan + (page * 8)] = vals[chan];
                seq_glob.set_array(seq).await;
                seq_glob.save().await;
            }

            if _shift {
                if chan % 2 == 0 {
                    //Odd number fader + shift
                    let mut seq_lenght = seq_length_glob.get_array().await;
                    seq_lenght[(chan / 2)] = ((vals[chan]) / 256) + 1;
                    seq_length_glob.set_array(seq_lenght).await;
                    seq_length_glob.save().await;
                }

                if chan % 2 == 1 {
                    //Odd number fader + shift
                    div.set(vals[chan] / 170 + 1).await;
                    info!("{}", (vals[chan] / 170 + 1));
                }
            }
            led_flag_glob.set(true).await;
        }
    };

    let fut3 = async {
        //Short button presses
        loop {
            let chan = buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get_array().await;
            let _shift = buttons.is_shift_pressed();
            let page = page_glob.get().await;
            if !_shift {
                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                gateseq_glob.set_array(gateseq).await;
                gateseq_glob.save().await;
                led_flag_glob.set(true).await;
            }

            if _shift {
                page_glob.set(chan).await;
                led_flag_glob.set(true).await;
            }
        }
    };

    let fut4 = async {
        loop {
            let colours = [
                (243, 191, 78),
                (188, 77, 216),
                (78, 243, 243),
                (250, 250, 250),
            ];
            app.delay_millis(10).await;
            //if buttons.is_shift_pressed().await;
            if buttons.is_shift_pressed() {
                let page = page_glob.get().await;
                let mut bright = 75;
                for n in 0..=7 {
                    if n == page {
                        bright = 150;
                    } else {
                        bright = 75;
                    }
                    led.set(n, Led::Button, colours[n / 2], bright);
                }
            }

            let led_flag = led_flag_glob.get().await;
            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get().await;
                let gateseq = gateseq_glob.get_array().await;
                let seq_length = seq_length_glob.get_array().await; //use this to highlight active notes
                let seq = seq_glob.get_array().await;
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
                    led.set(n, Led::Top, colour, (seq[n + (page * 8)] / 16) as u8 / 2);
                    led.set(
                        n,
                        Led::Bottom,
                        colour,
                        (255 - (seq[n + (page * 8)] / 16) as u8) / 2,
                    );
                    if gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, colour, 75);

                        //led.set(n, Led::Bottom , colour, 0);
                    }
                    if !gateseq[n + (page * 8)] {
                        led.set(n, Led::Button, colour, 50);
                        //led.set(n, Led::Bottom , colour, 0);
                    }

                    let index = seq_length[page / 2] as usize - (page % 2 * 8);
                    //info!("{}", index);

                    if n >= index || index > 16 {
                        led.set(n, Led::Button, colour, 0);
                    }

                    led.set(
                        (clockn % seq_length[page / 2] as usize) % 8,
                        Led::Button,
                        (255, 0, 0),
                        100,
                    );
                }

                led.set(page, Led::Bottom, colour, 255);

                led_flag_glob.set(false).await;
            }
        }
    };

    let fut5 = async {
        loop {
            let gateseq = gateseq_glob.get_array().await;
            let seq_length = seq_length_glob.get_array().await;
            let mut clockn = clockn_glob.get().await;
            let page = page_glob.get().await;
            let reset = clk.wait_for_tick(6).await;
            clockn += 1;
            if reset {
                clockn = 0;
                clockn_glob.set(clockn).await;
            }
            if !reset {
                clockn_glob.set(clockn).await;
                //led.set((clockn % seq_length[page / 2] as usize) % 8, Led::Button, (255, 0, 0), 100 );

                let seq = seq_glob.get_array().await;
                for n in 0..=3 {
                    let clkindex = ((clockn % seq_length[n] as usize) + (n * 16));
                    
                    if gateseq[clkindex] {
                        gate_out[n].set_high().await;
                        cv_out[n].set_value(seq[clkindex] / 4); //only update CV out on active step
                        midi[n]
                            .send_note_on((seq[clkindex] / 170) as u8 + 60, 4095)
                            .await;

                        //gate_flag_glob[n].set(true).await;

                        //app.delay_millis(gatet).await;
                        //gate_out[n].set_low().await;
                    } else {
                        gate_out[n].set_low().await;

                        midi[n]
                            .send_note_off((seq[clkindex] / 170) as u8 + 60)
                            .await

                        //app.delay_millis(gatet).await;
                        //led_flag_glob.set(true).await;
                    }
                    //led_flag_glob.set(true).await;
                }

                app.delay_millis(gatet).await;
                for n in 0..=3 {
                    let clkindex = (clockn % seq_length[n] as usize) + (n * 16);
                    if gateseq[clkindex] {
                        //gate_out[n].set_high().await;
                        //gate_flag_glob[n].set(true).await;

                        //app.delay_millis(gatet).await;
                        gate_out[n].set_low().await;

                        midi[n]
                            .send_note_off((seq[clkindex] / 170) as u8 + 60)
                            .await
                    }

                    led_flag_glob.set(true).await;
                }
                //clockn += 1;
            }
        }
    };

    join5(fut1, fut2, fut3, fut4, fut5).await;
}
