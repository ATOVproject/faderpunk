//To Do:

//add led feedback for the fader position
//add quantizer
//add per channel gate seq_lenght
//add MIDI


use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::{join3, join4, join5};
use embassy_rp::pac::dma::vals::TreqSel;
use embassy_sync::channel;
use embassy_time::Duration;

use crate::app::{App, Led, Range};

pub const CHANNELS: usize = 8;
pub const PARAMS: usize = 1;

pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
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


    let clockn_glob = app.make_global(0);
    let gatet = 20;

    let cv_out = [app.make_out_jack(0, Range::_0_10V).await, app.make_out_jack(2, Range::_0_10V).await, app.make_out_jack(4, Range::_0_10V).await, app.make_out_jack(6, Range::_0_10V).await];
    //let cv1 = app.make_out_jack(2, Range::_0_10V).await;
    let gate_out = [app.make_gate_jack(1, 4095).await, app.make_gate_jack(3, 4095).await,  app.make_gate_jack(5, 4095).await,  app.make_gate_jack(7, 4095).await];


    let seq_glob = app.make_global([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let gateseq_glob = app.make_global([true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true, true]);

    //let mut latched_glob = app.make_global([true, true, true, true, true, true, true, true]);

    let seq_length_glob = app.make_global([13, 16, 16, 16]);
    let page_glob = app.make_global(0);
    let led_flag_glob = app.make_global(true);

    let gate_flag_glob = app.make_global([false, false, false, false]);

    //let seq_init = faders.get_values();
    //seq_glob.set(seq_init).await;



    let fut1 = async {
        loop { // do the slides here
            app.delay_millis(1).await;


        }
    };

    let fut2 = async {
        loop {
            let chan = faders.wait_for_any_change().await;
            let vals = faders.get_values();
            let page = page_glob.get().await;
            let mut seq = seq_glob.get().await;
            let _shift = buttons.is_shift_pressed();

            if !_shift {
                seq[chan + (page * 8)] = vals[chan];
                seq_glob.set(seq).await;
                
            }

            if _shift {
                if chan % 2 == 0 {
                    let mut seq_lenght = seq_length_glob.get().await;
                    seq_lenght[(chan / 2)] = ((vals[chan]) / 256) + 1;
                    seq_length_glob.set(seq_lenght).await;
                    led_flag_glob.set(true).await;
                }

                if chan % 2 == 1 {

                }
                                
            }
        }
        
    };

    let fut3 = async { //Short button presses
        loop {
            let chan= buttons.wait_for_any_down().await;
            let mut gateseq = gateseq_glob.get().await;
            let _shift = buttons.is_shift_pressed();
            let page = page_glob.get().await;
            if !_shift {
                gateseq[chan + (page * 8)] = !gateseq[chan + (page * 8)];
                gateseq_glob.set(gateseq).await;
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
            app.delay_millis(1).await;
            let led_flag = led_flag_glob.get().await;
            if led_flag { // LED stuff
                let page = page_glob.get().await;
                let gateseq = gateseq_glob.get().await;
                let seq_length = seq_length_glob.get().await; //use this to highlight active notes
                let mut colour = (243, 191, 78);
                
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
                    if gateseq[n + (page * 8)] {
                        led.set(n, Led::Button , colour, 100);
                        led.set(n, Led::Bottom , colour, 0);
                    }
                    if !gateseq[n + (page * 8)] {
                        led.set(n, Led::Button , colour, 50);
                        led.set(n, Led::Bottom , colour, 0);
                        
                    }

             
                    let index = seq_length[page/2] as usize - (page % 2 * 8);
                    //info!("{}", index);

                    if n >= index || index > 16{
                        led.set(n, Led::Button , colour, 0);
                    }
                    
                }
                led.set(page, Led::Bottom , colour, 75);
                led_flag_glob.set(false).await;
            }
        }
    };


    let fut5 = async {
        loop {
            let gateseq = gateseq_glob.get().await;
            let seq_length = seq_length_glob.get().await;
            let mut clockn = clockn_glob.get().await;
            let page = page_glob.get().await;
            clk.wait_for_tick(24).await;
            clockn += 1;
            
            clockn_glob.set(clockn).await;
            led.set((clockn % seq_length[page / 2] as usize) % 8, Led::Button , (255, 0, 0), 100);
            
            let seq = seq_glob.get().await;
            for n in 0..=3 {
                let clkindex = ((clockn % seq_length[n] as usize)  + (n * 16));
                cv_out[n].set_value(seq[clkindex] / 5);
                if gateseq[clkindex]{
                    gate_out[n].set_high().await;
                    //gate_flag_glob[n].set(true).await;
                
                    
                    //app.delay_millis(gatet).await;
                    //gate_out[n].set_low().await;
                }
                else {
                    gate_out[n].set_low().await;
                    //app.delay_millis(gatet).await;
                    //led_flag_glob.set(true).await;
                }
                //led_flag_glob.set(true).await;
            }

            app.delay_millis(gatet).await;
            for n in 0..=3 {
                let clkindex = (clockn % seq_length[n] as usize)  + (n * 16);
                if gateseq[clkindex]{
                    //gate_out[n].set_high().await;
                    //gate_flag_glob[n].set(true).await;
                
                    
                    //app.delay_millis(gatet).await;
                    gate_out[n].set_low().await;
                }
                led_flag_glob.set(true).await;
            }


        

            /*
            if slide {
            // set high but not low, do not set the voltage, raise slide flag for slide to happen in timed loop
            
            }
             */
            
            }
    };

    join5(fut1, fut2, fut3, fut4, fut5).await;
}
