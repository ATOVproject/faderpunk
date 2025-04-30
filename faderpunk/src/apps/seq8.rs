//To Do:

//add led feedback for the fader position
//add quantizer
//add per channel gate seq_lenght
//add MIDI

use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::join5;

use crate::app::{App, Arr, Led, Range};

pub const CHANNELS: usize = 8;

app_config!(
    config("Sequencer", "4 x 16 step CV/Gate sequencer");

    params(
        
);

    storage(
        seq_glob => (Arr<u16, 64>, Arr([0; 64])),
        gateseq_glob => (Arr<bool, 64>, Arr([true; 64])),
        seq_length_glob => (Arr<u16, 4>, Arr([15; 4])),
    );
);

pub async fn run(app: App<'_, CHANNELS>, ctx: &AppContext<'_>) {

    let seq_glob = &ctx.storage.seq_glob;
    let gateseq_glob = &ctx.storage.gateseq_glob;
    let seq_length_glob = &ctx.storage.seq_length_glob;

    // let seq_glob = stor_seq.get().await;
    // let gateseq_glob = stor_gate.get().await;
    // let seq_length_glob = stor_length.get().await;

    
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

    


    //let mut latched_glob = app.make_global([true, true, true, true, true, true, true, true]);

    let page_glob = app.make_global(0);
    let led_flag_glob = app.make_global(true);
    let lenght_flag = app.make_global(false);
    let latched_glob = app.make_global([false; 8]);
    let save_flag = app.make_global(false);

    let div = app.make_global(1);
    let mut shif_old = false;
    let gate_flag_glob = app.make_global([false, false, false, false]);
    let mut shift_old = false;

    let mut count = 0;
    let fut1 = async {
        loop {
            // do the slides here
            app.delay_millis(1).await;
            count += 1;
            if count == 20000 && save_flag.get().await {
                count = 0;
                seq_glob.save().await;
                seq_length_glob.save().await;
                gateseq_glob.save().await;
                save_flag.set(false).await
            }
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
            let mut seq = seq_glob.get().await;
            let _shift = buttons.is_shift_pressed();
            let mut latched = latched_glob.get().await;
            let mut seq_lenght = seq_length_glob.get().await;
            if return_if_close(vals[chan], seq.0[chan + (page * 8)]) && !_shift {
                latched[chan] = true;
                latched_glob.set(latched).await;
            }

            if !_shift && chan < 8 && latched[chan] {
                if latched[chan] {
                    seq.0[chan + (page * 8)] = vals[chan];
                    //info!("{}", seq[chan + (page * 8)]);
                    seq_glob.set(seq).await;
                    save_flag.set(true).await;
                    //seq_glob.save().await;
                }
            }

            if vals[0] / 256 + 1 == seq_lenght.0[page / 2] && _shift {
                latched[0] = true;
                latched_glob.set(latched).await;
                //info!("latching!");
            }

            if _shift {
                // add check for latching
                if chan == 0 && latched[0] {
                    //fader 1 + shift
                    seq_lenght.0[(page / 2)] = ((vals[0]) / 256) + 1;
                    //info!("{}", seq_lenght[page / 2]);
                    seq_length_glob.set(seq_lenght).await;
                    save_flag.set(true).await;
                    //seq_length_glob.save().await;
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
            let mut gateseq = gateseq_glob.get().await;
            let _shift = buttons.is_shift_pressed();
            let page = page_glob.get().await;
            if !_shift {
                gateseq.0[chan + (page * 8)] = !gateseq.0[chan + (page * 8)];
                gateseq_glob.set(gateseq).await;
                //gateseq_glob.save().await;
                save_flag.set(true).await;
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
                let seq_length = seq_length_glob.get().await;
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
                    if n == clockn as u16 % seq_length.0[page / 2] {
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

            let led_flag = led_flag_glob.get().await;
            if !buttons.is_shift_pressed() {
                // LED stuff
                let page = page_glob.get().await;
                let gateseq = gateseq_glob.get().await;
                let seq_length = seq_length_glob.get().await; //use this to highlight active notes
                let seq = seq_glob.get().await;
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
            let gateseq = gateseq_glob.get().await;
            let seq_length = seq_length_glob.get().await;
            let mut clockn = clockn_glob.get().await;
            let page = page_glob.get().await;
            let reset = clk.wait_for_tick(6).await;
            clockn += 1;
            if reset {
                clockn = 0;
                clockn_glob.set(clockn).await;
                info!("reset")
            }
            if !reset {
                clockn_glob.set(clockn).await;
                //led.set((clockn % seq_length.0[page / 2] as usize) % 8, Led::Button, (255, 0, 0), 100 );

                let seq = seq_glob.get().await;
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

    join5(fut1, fut2, fut3, fut4, fut5).await;
}

fn return_if_close(a: u16, b: u16) -> bool {
    if a.abs_diff(b) < 100 {
        true
    } else {
        false
    }
}
