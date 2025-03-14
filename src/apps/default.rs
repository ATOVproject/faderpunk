use defmt::info;
use crate::config::{Config, Curve, Param};
use embassy_futures::join::join3;
// use minicbor::encode;

use crate::app::App;

// API ideas:
// - app.wait_for_midi_on_channel

pub const CHANNELS: usize = 1;

pub static APP_CONFIG: Config<1> = Config::default().add_param(Param::Curve {
    name: "Curve",
    default: Curve::Linear,
    variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
});

// let mut buffer = [0u8; 128];
// let x = encode(APP_CONFIG.params(), buffer.as_mut()).unwrap();

pub async fn run(app: App<CHANNELS>) {
    let config = APP_CONFIG.to_runtime_config().await;
    let curve = config.get_curve_at(0);

    let glob_muted = app.make_global(false);
    let glob_midi = app.make_global(0);
    let mut oldmidi = 0;

    let jack = app.make_out_jack(0).await;

    app.set_led(0, (0, 255, 0), 50).await;
    let mut maxval= [4096];


    let fut1 = async {

        let mut waiter = app.make_waiter();
        loop {
            
            waiter.wait_for_fader_change(0).await;
            if app.is_shift_pressed() {
                maxval = app.get_fader_values();
                info!("shifted!");
            }
            else{
                let muted = glob_muted.get().await;
                if !muted {
                    let vals = app.get_fader_values();
                    jack.set_value_with_curve(curve, vals[0]);
                    glob_midi.set(vals[0]).await;
                }
            }
            
        }
    };

    let fut2 = async {

        loop {
            app.delay_millis(100).await;
            let midival: u16 = glob_midi.get().await;
            if midival / 32 != oldmidi /32 { //detect if new midi need to be sent
                app.midi_send_cc(app.channels[0], midival).await;
                oldmidi = midival;
                info!("midi sent {}", midival);            
            }
            
        }
    };

    let fut3 = async {
        let mut waiter = app.make_waiter();
        loop {
            waiter.wait_for_button_down(0).await;
            let muted = glob_muted.toggle().await;
            if muted {
                jack.set_value(0);
                app.set_led(0, (255, 0, 0), 50).await;
                glob_midi.set(0).await;
            }
            else{
                app.set_led(0, (0, 255, 0), 50).await;
                let vals = app.get_fader_values();
                glob_midi.set(vals[0]).await;
                jack.set_value_with_curve(curve, vals[0]);
            }
        }
    };


    join3(fut1, fut2, fut3).await;
}
