use config::{Config, Curve, Param};
use defmt::info;
use embassy_futures::join::join3;

use crate::app::{App, Led, Range, StorageSlot};

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 3;

// TODO: How to add param for midi-cc base number that it just works as a default?
pub static CONFIG: Config<PARAMS> = Config::new("Default", "16n vibes plus mute buttons")
    .add_param(Param::Curve {
        name: "Curve",
        default: Curve::Linear,
        variants: &[Curve::Linear, Curve::Exponential, Curve::Logarithmic],
    })
    .add_param(Param::Int {
        name: "Midi channel",
        default: 1,
        min: 1,
        max: 16,
    });

const LED_COLOR: (u8, u8, u8) = (0, 200, 150);
const BUTTON_BRIGHTNESS: u8 = 75;

pub async fn run(app: App<CHANNELS>) {
    let config = CONFIG.as_runtime_config().await;
    // TODO: Maybe rename: get_curve_from_param(idx)
    let midi_channel = config.get_int_at(1) as u8;

    let buttons = app.use_buttons();
    let faders = app.use_faders();
    let leds = app.use_leds();
    let midi = app.use_midi(midi_channel - 1);
    let mut clock = app.use_clock();

    let midi_notes = [60, 65, 67, 70, 78];

    let mut glob_muted = app.make_global_with_store(false, StorageSlot::A);
    glob_muted.load().await;

    let mut divider_glob = app.make_global_with_store(6, StorageSlot::B);
    divider_glob.load().await;

    let mut toggle_glob = app.make_global_with_store(false, StorageSlot::C);
    toggle_glob.load().await;
    
    let note_ind_glob = app.make_global(0);

    let muted = glob_muted.get().await;
    leds.set(
        0,
        Led::Button,
        LED_COLOR,
        if muted { 0 } else { BUTTON_BRIGHTNESS },
    );

    let jack = app.make_out_jack(0, Range::_0_10V).await;
    let fut1 = async {
        loop {
            let mut divider = divider_glob.get().await;
            clock.wait_for_tick(divider).await;
            
            let note_ind = note_ind_glob.get().await;

            let muted = glob_muted.get().await;
            //let shift = buttons.is_shift_pressed();
            info!("button {}", muted);
            if !muted {
                midi.send_note_on(midi_notes[note_ind], 127).await;
                app.delay_millis(10).await;
                midi.send_note_off(midi_notes[note_ind]).await;
            }
            

            
        }
    };

    let fut2 = async {
        loop {
           faders.wait_for_change(0).await;
           let shift = buttons.is_shift_pressed();
           let vals = faders.get_values();
           if shift {
            let divider = vals[0] / 178 + 1;
            divider_glob.set(divider as usize).await;
            divider_glob.save().await;
           }
           else {
               let note_ind = vals[0]/ 819;
               note_ind_glob.set(note_ind as usize).await;
           }
            

        }
    };

    let fut3 = async {
        loop {
            buttons.wait_for_down(0).await;
            let shift = buttons.is_shift_pressed();
            if !shift {
                let muted = glob_muted.toggle().await;
                glob_muted.save().await;
                if muted {
                    leds.set(0, Led::Button, LED_COLOR, 0);
                    leds.set(0, Led::Top, LED_COLOR, 0);
                    leds.set(0, Led::Bottom, LED_COLOR, 0);
                } else {
                    leds.set(0, Led::Button, LED_COLOR, BUTTON_BRIGHTNESS);
                }
            }
            else {
                toggle_glob.toggle().await;
            }
        
        }            
    };

    join3(fut1, fut2, fut3).await;
}
