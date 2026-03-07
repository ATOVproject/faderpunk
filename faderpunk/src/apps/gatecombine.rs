//! # Gate Combine app
//! 
//! Combines two other output gates or triggers from other apps into a single combined gate signal, using binary logic.
//! The combined gate signal is then passed through a probabilistic gate that is controlled by the app's fader.
//! If the combined signal goes high, and the probablistic gate allows it through, the output gate will go high,
//! and stay high until the combined signal goes low, else the output will be low.
//!
//! Created by Richard Smith (@phommed on Faderpunk Discord) in February 2026. 
//! 
use embassy_futures::{
    join::{join5}, select::{select, select3}
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    APP_MAX_PARAMS, AppIcon, Brightness, Color, Config, GLOBAL_CHANNELS,
         latch::LatchLayer,
        Param, Value, ext::FromValue};

use serde::{Deserialize, Serialize};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent };

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 6;

const LED_BRIGHTNESS: Brightness = Brightness::High;

// Sampled input gate jack changed state must remain the same state for given number of milliseconds to change Gate Combine output
// Intention is to smooth out micro-timing differences between sampled output gates, or when chaining successive Gate Combine apps  
const LATCHED_GATE_CHANGE_MILLIS: u32 = 3;

// App configuration visible to the configurator
pub static CONFIG: Config<PARAMS> = Config::new(
    "Gate Combine",
    "Adds two gate or trigger outputs from other channels together using binary logic",
    Color::SkyBlue,
    AppIcon::KnobRound,
)
.add_param(Param::bool { name: "Enable Channel A" })
.add_param(Param::i32 { name: "Channel A Jack", min: 1, max: GLOBAL_CHANNELS as i32 })
.add_param(Param::bool { name: "Enable Channel B" })
.add_param(Param::i32 { name: "Channel B Jack", min: 1, max: GLOBAL_CHANNELS as i32 })
.add_param(Param::Color {
    name: "Color",
    variants: &[
        Color::Blue,
        Color::Green,
        Color::Rose,
        Color::Orange,
        Color::Cyan,
        Color::Pink,
        Color::Violet,
        Color::Yellow,
    ],
})
.add_param(Param::Enum {
    name: "Combine Mode",
    variants: &["OR", "AND", "XOR", "NOR", "NAND", "XNOR"]});

pub struct Params {
    // Will be added if = true
    channel_a_enabled: bool,
    // Output jack number 1 - GLOBAL_CHANNELS to be sampled
    channel_a_gate_jack_num: i32,
    // Will be added if = true
    channel_b_enabled: bool,
    // Output jack number 1 - GLOBAL_CHANNELS to be sampled
    channel_b_gate_jack_num: i32,
    // LED colour
    color: Color,
    // Output combination mode
    combine_mode: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            channel_a_enabled: false,
            channel_a_gate_jack_num: 1,
            channel_b_enabled: false,
            channel_b_gate_jack_num: 1,
            color: Color::Yellow,
            combine_mode: 0, // OR
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            channel_a_enabled: bool::from_value(values[0]),
            channel_a_gate_jack_num: i32::from_value(values[1]),
            channel_b_enabled: bool::from_value(values[2]),
            channel_b_gate_jack_num: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
            combine_mode: usize::from_value(values[5]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.channel_a_enabled.into()).unwrap();
        vec.push(self.channel_a_gate_jack_num.into()).unwrap();
        vec.push(self.channel_b_enabled.into()).unwrap();
        vec.push(self.channel_b_gate_jack_num.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.combine_mode.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    channel_a_mute_saved: bool,
    channel_b_mute_saved: bool,
    prob_saved: u16,
}
impl Default for Storage {
    fn default() -> Self {
        Self {
            channel_a_mute_saved: false,
            channel_b_mute_saved: false,
            prob_saved: 4096,
        }
    }
}

impl AppStorage for Storage {}

// Wrapper task - required for all apps
#[embassy_executor::task(pool_size = 16/CHANNELS)]
pub async fn wrapper(app: App<CHANNELS>, exit_signal: &'static Signal<NoopRawMutex, bool>) {
    let param_store = ParamStore::<Params>::new(app.app_id, app.layout_id);
    let storage = ManagedStorage::<Storage>::new(app.app_id, app.layout_id);

    param_store.load().await;
    storage.load().await;

      let app_loop = async {
        loop {
            select3(
                run(&app, &param_store, &storage),
                param_store.param_handler(),
                storage.saver_task(),
            )
            .await;
        }
    };

    select(app_loop, app.exit_handler(exit_signal)).await;
}

// Main app logic
pub async fn run(app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {


    // first_channel and second_channel params are converted to usize in range 0 - (GLOBAL_CHANNELS-1)
    let (channel_a_enabled, channel_a_gate_jack_num, channel_b_enabled, channel_b_gate_jack_num, led_color, combine_mode) = params
    .query(|p| {
        (
            p.channel_a_enabled,
            p.channel_a_gate_jack_num,
            p.channel_b_enabled,
            p.channel_b_gate_jack_num,
            p.color,
            p.combine_mode,
        )
    });

    let channel_a_safe = (channel_a_gate_jack_num.clamp(1, GLOBAL_CHANNELS as i32) - 1) as usize;
    let channel_b_safe = (channel_b_gate_jack_num.clamp(1, GLOBAL_CHANNELS as i32) - 1) as usize;

    let output = app.make_gate_jack(0, 4095).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();
    let die = app.use_die();

    let channel_a_mute_glob = app.make_global(storage.query(|s| s.channel_a_mute_saved));
    let channel_b_mute_glob = app.make_global(storage.query(|s| s.channel_b_mute_saved));
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    // Set up initial state of LED button
    if channel_a_mute_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let mut old_out_gate_was_high = false;

    let main_fut = async {
        
        let mut gate_a_unchanged_counter = 0u32;
        let mut gate_b_unchanged_counter = 0u32;
        let mut last_gate_a_is_high = false;
        let mut last_gate_b_is_high = false;
        let mut latched_gate_a_is_high = false;
        let mut latched_gate_b_is_high = false;

        loop {
            app.delay_millis(1).await;

            latched_gate_change(channel_a_safe, &mut gate_a_unchanged_counter, &mut last_gate_a_is_high, &mut latched_gate_a_is_high);
            latched_gate_change(channel_b_safe, &mut gate_b_unchanged_counter, &mut last_gate_b_is_high, &mut latched_gate_b_is_high);
            
            let channel_a_active = channel_a_enabled && !channel_a_mute_glob.get();
            let channel_b_active = channel_b_enabled && !channel_b_mute_glob.get();
            let channel_a_use =  channel_a_active && app.start_channel != channel_a_safe;
            let channel_b_use = channel_b_active && app.start_channel != channel_b_safe;
            let a_is_high  = if channel_a_use { 
                    latched_gate_a_is_high
                } else {
                    false
                };
            let b_is_high = if channel_b_use { 
                    latched_gate_b_is_high
                } else {
                    false
                };
            let mut out_is_high:bool = if combine_mode == 0 {
                // OR
                a_is_high | b_is_high    
            } else if combine_mode == 1 {
                // AND
                a_is_high & b_is_high    
            } else if combine_mode == 2 {
                // XOR
                a_is_high ^ b_is_high
            } else if combine_mode == 3 {
                // NOR 
                !(a_is_high | b_is_high)
            } else if combine_mode == 4 {
                // NAND
                !(a_is_high & b_is_high)
            } else if combine_mode == 5 {
                // XNOR
                !(a_is_high ^ b_is_high)
            } else { 
                false 
            };

            // Apply probabilistic gate
                
            let prob = storage.query(|s| s.prob_saved); // Get gate probability from fader 0 - 4095
            if out_is_high && !old_out_gate_was_high {
                // Combined output has just gone high.

                let rand_val = die.roll();
                // The higher the probability fader, more chance there is of gate pasing through
                if rand_val >= prob {
                    // bad luck, gate must stay low this time
                    out_is_high = false;
                }
            }

            old_out_gate_was_high = out_is_high;

            if out_is_high {
                output.set_high().await;
                leds.set(0, Led::Top, led_color, LED_BRIGHTNESS);
            } else {
                output.set_low().await;
                leds.unset(0, Led::Top);
            }

            // Update bottom LED to show trigger probability when on main latch layer
            match glob_latch_layer.get() {
                LatchLayer::Main => {   
                        let pos: u8 = (prob / 8).clamp(0, 255) as u8;
                        leds.set(0, Led::Bottom, Color::Blue, Brightness::Custom(pos));        
                }
                _ => {
                    leds.unset(0, Led::Bottom);
                }
            };

       }
    };

     let fader_fut = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change_at(0).await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.prob_saved),
                LatchLayer::Alt => 0,
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.prob_saved = new_value);
                    }
                    LatchLayer::Alt => {}
                    LatchLayer::Third => {}
                }
            }
        }
    };

    let btn_fut = async {
        loop {
            buttons.wait_for_down(0).await;
            if !buttons.is_shift_pressed() {
                // First channel mute
                let muted = storage.modify_and_save(|s| {
                    s.channel_a_mute_saved = !s.channel_a_mute_saved;
                    s.channel_a_mute_saved
                });
                channel_a_mute_glob.set(muted);
                if muted {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
            } else {
                // Second channel mute
                let muted = storage.modify_and_save(|s| {
                    s.channel_b_mute_saved = !s.channel_b_mute_saved;
                    s.channel_b_mute_saved
                });
                channel_b_mute_glob.set(muted);
                if muted {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
            }
        };       
    };

    let shift_fut = async {
        loop {
            app.delay_millis(1).await;

            glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));

            // Change state of button when shift is pressed or released to show correct active state of first or second added channels
            if !buttons.is_shift_pressed() {
                let muted = storage.query(|s| s.channel_a_mute_saved);
                if muted {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
            } else {
                let muted = storage.query(|s| s.channel_b_mute_saved);
                if muted {
                    leds.unset(0, Led::Button);
                } else {
                    leds.set(0, Led::Button, led_color, Brightness::Mid);
                }
            }
            
        };
    };

    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;
                    channel_a_mute_glob.set(storage.query(|s| s.channel_a_mute_saved));
                    channel_b_mute_glob.set(storage.query(|s| s.channel_b_mute_saved));
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join5(main_fut, fader_fut, btn_fut, shift_fut, scene_handler).await;

}

fn latched_gate_change(channel_safe: usize, gate_unchanged_counter: &mut u32, last_gate_is_high: &mut bool, latched_gate_is_high: &mut bool) {
    let jack_is_now_high = App::<CHANNELS>::get_out_global_gate_jack_is_high(channel_safe);
    if *last_gate_is_high == jack_is_now_high {
        *gate_unchanged_counter += 1;
    } else {
        *last_gate_is_high = !*last_gate_is_high;
        *gate_unchanged_counter = 0;
    }
    if *latched_gate_is_high != jack_is_now_high && *gate_unchanged_counter > LATCHED_GATE_CHANGE_MILLIS {
        *latched_gate_is_high = jack_is_now_high;
    }
}