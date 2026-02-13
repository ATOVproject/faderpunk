//! # CV Combine app
//! 
//! Precision CV adder / combiner / quantizer of one or two other output variable CV jacks belonging to other apps
//!
//! Created by Richard Smith (@rjsmith on GitHub) in February 2026. 
//! 
//! This app is able to sum the output voltages of up to two other specified output jacks belonging to other apps in the same layout.
//! It simulates the behaviour of Eurorack precision adder / CV math modules.
//! 
//! The app combines the one or two sampled CV output jacks together, optionally quantises the sum, optionally applies a voltage offset, then re-scales the output signal to the required output voltage range.
//! 
//! If the final voltage value exceed the min and max of the output Range, the summed CV will be hardclipped to the min or max of the Range.
//! 
//! This app does not output any MIDI events.
//! 
//! ## Combine Modes
//! 
//! The app has 5 "Combine Modes" for combining the CV from the two sampled jacks:
//! 1. A + B + offset: Sums the CV from the two jacks together,
//! 2. A - B + offset: Subtracts the CV of the second jack from the first,
//! 3. Max + offset: Outputs the higher of the two CVs,
//! 4. Min + offset: Outputs the lower of the two CVs,
//! 5. Average + offset: Outputs the average of the two CVs (if both channels active)
//! 
//! ## Hardware Mapping
//! 
//! | Control | Function | + Shift | + Fn
//! |---------|----------|---------|------|
//! | Jack 1  | CV out   | N/A     | N/A  |
//! | Fader 1 | Offset   | Divisor | N/A. | 
//! | LED 1 Top | CV output level | Divisor | N/A
//! | LED 1 Bottom | Offset (incl. divisor) | N/A | N/A
//! | Fn 1    | Short press: Toggle Channel A, Long press: Toggle Offset | Short press: Toggle Channel B| N/A |
//! 
//! The fader sets an CV offset which is applied to the combined CV output after the quantiser.
//! By default, the offset is an effective range of -5V to + 5V, in steps of 1V. So if you are combining two v/o pitch signals, this
//! shifts the post-quantized signal from -5 to +5 octaves. The bottom LED shows the level of the offset, with the midpoint (off) representing zero offset, fully lit (blue) representing +5V, and fully lit (red) representing -5V.
//!
//! Shift + Fader sets a divisor for the offset (in range 1 at the bottom to 12 at the top) so you can use fractions of a volt as an offset. When the divisor is 12, and the CV is v/o pitch, the offset is in terms of semitones.
//! 
//! The offset can be toggled on and off by Shift + long-pressing the button.
//! 
//! ## Scene Storage
//! The app saves the mute state of the two channels and the offset voltage and divisor in scenes.
//! 
//! ## Usage Tips
//! 
//! ### Steevio Sequencing
//! Replicate the magic sequencing techniques of the Welsh modular musician, Steevio!:
//! 1. Set up a "Sequencer" 8-channel app with two note patterns with different lengths and tempo (e.g. 5 and 7 steps).
//! 2. Place an "CV Combine" app somewhere else on the layout, configuring its "A" and "B" Jack channels to the first two "CV Output" jacks of the Sequencer (channels 1 & 3 on the Sequencer).
//! 3. Set the Combine's output range to 0-10V, turn on Quantize output (to match the fixed 0-10V output range of the Sequencer app).
//! 
//! ### Muting 
//! Mute and unmute the CV Combine's "A" and "B" channels to bring in either pattern
//! 
//! ### Combine Modes
//! Try out the different Combine Modes for different variations in how the two patterns interact. 
//! 
//! ### Octave and semitone shifting
//! Add some octave shifts (+5 to -5 octave offsets) by moving the bipolar offset fader (no offset in the middle of its range)
//! Hold Shift and experiment with different offset divisors. 
//! With a divisor of 12 (fader at top of its range), the offset is in semitones
//! 
//! ### Mix LFOs and Control CVs
//! Configure the CV Combine to mix an LFO and a Control app together, or two LFOs set to different frequencies, for more complex modulation patterns.
//! (Disable the quantizer in this case to preserve the smoothness of the LFO signal)
//! 
//! ### Scene Switching
//! Save different scene settings for a CV Combine app with different offsets or channel mutes, then switch betwen them during a performance.
//! If you also change the scene settings of the upstream CV generating apps, you can get some really interesting switched melodies and modulation.
//! 
use embassy_futures::{
    join::{join5}, select::{select, select3}
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use libfp::{
    APP_MAX_PARAMS, AppIcon, Brightness, Color, Config, GLOBAL_CHANNELS,     latch::LatchLayer,
Param, Range, Value, ext::FromValue};

use libm::roundf;
use serde::{Deserialize, Serialize};

use crate::app::{App, AppParams, AppStorage, Led, ManagedStorage, ParamStore, SceneEvent };

pub const CHANNELS: usize = 1;
pub const PARAMS: usize = 8;

const _5V:i16 = 2047;
const _10V:i16 = 4095;
const V_PER_OCTAVE:f32 = 409.5;

// App configuration visible to the configurator
pub static CONFIG: Config<PARAMS> = Config::new(
    "CV Combine",
    "Adds two CV outputs from other channels, with octave offset and optional global quantization",
    Color::Yellow,
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
.add_param(Param::Range {
    name: "Range",
    variants: &[Range::_0_10V, Range::_0_5V, Range::_Neg5_5V],
}).add_param(Param::bool { name: "Quantize output" })
.add_param(Param::Enum {
    name: "Combine Mode",
    variants: &["A+B", "A-B", "Max", "Min", "Average"]});

pub struct Params {
    // Will be added if = true
    channel_a_enabled: bool,
    // Output jack number 1 - GLOBAL_CHANNELS to be sampled
    channel_a_jack_num: i32,
    // Will be added if = true
    channel_b_enabled: bool,
    // Output jack number 1 - GLOBAL_CHANNELS to be sampled
    channel_b_jack_num: i32,
    // LED colour
    color: Color,
    // Output CV range
    range: Range,
    // Quantize output = true
    quantize: bool,
    // Output combination mode
    combine_mode: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            channel_a_enabled: false,
            channel_a_jack_num: 1,
            channel_b_enabled: false,
            channel_b_jack_num: 1,
            color: Color::Yellow,
            range: Range::_0_10V,
            quantize: false,
            combine_mode: 0, // A + B
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
            channel_a_jack_num: i32::from_value(values[1]),
            channel_b_enabled: bool::from_value(values[2]),
            channel_b_jack_num: i32::from_value(values[3]),
            color: Color::from_value(values[4]),
            range: Range::from_value(values[5]),
            quantize: bool::from_value(values[6]),
            combine_mode: usize::from_value(values[7]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.channel_a_enabled.into()).unwrap();
        vec.push(self.channel_a_jack_num.into()).unwrap();
        vec.push(self.channel_b_enabled.into()).unwrap();
        vec.push(self.channel_b_jack_num.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.range.into()).unwrap();
        vec.push(self.quantize.into()).unwrap();
        vec.push(self.combine_mode.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    channel_a_mute_saved: bool,
    channel_b_mute_saved: bool,
    offset_enabled_saved:bool,
    offset_voltage_saved: u16,
    offset_divisor_saved: u16,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            channel_a_mute_saved: false,
            channel_b_mute_saved: false,
            offset_enabled_saved: true,
            offset_voltage_saved: 0,
            offset_divisor_saved: 0,
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
    let (channel_a_enabled, channel_a_jack_num, channel_b_enabled, channel_b_jack_num, led_color, range, quantize, combine_mode) = params
    .query(|p| {
        (
            p.channel_a_enabled,
            p.channel_a_jack_num,
            p.channel_b_enabled,
            p.channel_b_jack_num,
            p.color,
            p.range,
            p.quantize,
            p.combine_mode,
        )
    });

    let channel_a_safe = (channel_a_jack_num.clamp(1, GLOBAL_CHANNELS as i32) - 1) as usize;
    let channel_b_safe = (channel_b_jack_num.clamp(1, GLOBAL_CHANNELS as i32) - 1) as usize;

    let output = app.make_out_jack(0, range).await;
    let fader = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let quantizer = app.use_quantizer(range);
    let channel_a_mute_glob = app.make_global(storage.query(|s| s.channel_a_mute_saved));
    let channel_b_mute_glob = app.make_global(storage.query(|s| s.channel_b_mute_saved));
    let glob_latch_layer = app.make_global(LatchLayer::Main);

    // Set up initial state of LED button
    if channel_a_mute_glob.get() {
        leds.unset(0, Led::Button);
    } else {
        leds.set(0, Led::Button, led_color, Brightness::Mid);
    }

    let main_fut = async {
       
       loop {
            app.delay_millis(1).await;

            let channel_a_active = channel_a_enabled && !channel_a_mute_glob.get();
            let channel_b_active = channel_b_enabled && !channel_b_mute_glob.get();

            // Prevent feedback loops by disabling the possibility to sample from the same channel that the app is outputting on
            let channel_a_use =  channel_a_active && app.start_channel != channel_a_safe;
            let channel_b_use = channel_b_active && app.start_channel != channel_b_safe;

            // Get output jack config to find the configured output CV Range
            // TODO: Take this out of this fast loop
            let a_jack_config = if channel_a_use { 
                        App::<CHANNELS>::get_out_jack_config(channel_a_safe).await
                } else {
                    None
                };
            let b_jack_config = if channel_b_use { 
                        App::<CHANNELS>::get_out_jack_config(channel_b_safe).await
                } else {
                    None
                };

            // Get sampled jack values and transform to voltage values according to their individual configured CV Range. 
            // If channel not active, treat as zero. If jack config not found (e.g. app unplugged), also treat as zero.
            let a_in_v: i16 = if channel_a_use {
                match a_jack_config {
                    Some(config) => { 
                        let raw = App::<CHANNELS>::get_out_global_jack_value(channel_a_safe);
                        config.range.jack_value_to_voltage_value(raw)
                    },
                    None => 0
                }
            } else {
                0
            };
            let b_in_v:i16 = if channel_b_use {
                    match b_jack_config {
                        Some(config) => { 
                        let raw = App::<CHANNELS>::get_out_global_jack_value(channel_b_safe);
                        config.range.jack_value_to_voltage_value(raw)
                        },
                        None => 0
                    }
            } else {
                0
            };
            let a_plus_5v = a_in_v + _5V;
            let b_plus_5v = b_in_v + _5V;    
            
            let mut out_v:i16 = if combine_mode == 0 {
                // Add
                a_plus_5v + b_plus_5v - _10V    
            } else if combine_mode == 1 {
                // Subtract
                a_plus_5v - b_plus_5v - _10V    
            } else if combine_mode == 2 {
                // Max
                if a_plus_5v > b_plus_5v { a_in_v } else { b_in_v }
            } else if combine_mode == 3 {
                // Min
                if channel_a_use && channel_b_use {
                    if a_plus_5v < b_plus_5v { a_in_v } else { b_in_v }
                } else if channel_a_use && !channel_b_use {
                    a_in_v
                } else if !channel_a_use && channel_b_use {
                    b_in_v
                } else {
                    0
                }
            } else if combine_mode == 4 {
                // Average
                // If both channels active, output their average, else either a or b only
                if channel_a_use && channel_b_use {
                    ((a_plus_5v + b_plus_5v) / 2) - _5V
                } else if channel_a_use && !channel_b_use {
                    a_in_v
                } else {
                    b_in_v
                }
            } else { 
                0 
            };


            // Optionally add additional octave or divided semitone offset
            let (offset_enabled, offset_voltage_saved, offset_divisor_saved) = storage.query(|s| (s.offset_enabled_saved, s.offset_voltage_saved, s.offset_divisor_saved));
            let mut offset_v = 0;
            let mut offset_divisor = 1;
            if offset_enabled {
                offset_divisor = if offset_divisor_saved > 0 { offset_divisor_saved } else { 1 };
                offset_v = calculate_offset_voltage(offset_voltage_saved, offset_divisor);
                out_v += offset_v;
            }

            // Update bottom LED to either show offset or divisor level
            match glob_latch_layer.get() {
                LatchLayer::Main => {   
                    if offset_enabled && offset_v > 0 {
                        let pos: u8 = (offset_v / 8).clamp(0, 255) as u8;
                        leds.set(0, Led::Bottom, Color::Blue, Brightness::Custom(pos));
                    } else if offset_enabled && offset_v < 0 {
                        let neg = ((offset_v.abs()) / 8).clamp(0, 255) as u8;
                        leds.set(0, Led::Bottom, Color::Rose, Brightness::Custom(neg));
                    } else {
                        leds.unset(0, Led::Bottom);
                    }           
                }
                LatchLayer::Alt => {
                    let divisor: u8 = (((11.0 / 4095.0) * offset_divisor as f32) as i16 + 1).clamp(1, 12) as u8;
                    leds.set(0, Led::Bottom, Color::Green, Brightness::Custom(21 * divisor)); // 255/12 is approx 21
                }
                _ => {}
            };

            // Clamp to output voltage range, negative voltages clamped to 0V if output range does not support negative voltages
            out_v = out_v.clamp(-_5V, _10V);
            let out_safe = match range {
                Range::_0_10V => out_v.clamp(0, _10V) as u16,
                Range::_0_5V => out_v.clamp(0, _5V) as u16 * 2,
                Range::_Neg5_5V => (out_v.clamp(-_5V, _5V) + _5V) as u16,
            };

            // Optionally quantize output CV to global quantizer scale
            let out: u16 = if quantize {
                let out_pitch = quantizer.get_quantized_note(out_safe).await;
                out_pitch.as_counts(range)
            } else {
                out_safe
            };

            output.set_value(out);
            leds.set(0, Led::Top, led_color, Brightness::Custom((out / 16) as u8));
       

            // Change state of button when shift is pressed or released to show correct active state of channels A & B
            glob_latch_layer.set(LatchLayer::from(buttons.is_shift_pressed()));
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

       }
    };

    // Returns offset voltage in range -2047 to  + 2047 (ie. -5V to +5V) 
    fn calculate_offset_voltage(offset_voltage_saved: u16, offset_divisor: u16) -> i16 {
        // Map divisor saved value to 1 - 12
        let divisor: i16 = (((11.0 / 4095.0) * offset_divisor as f32) as i16 + 1).clamp(1, 12);
        let offset = (offset_voltage_saved as f32 / 409.5)  - 5.0; // in range -5.0 to +5.0V (-2047 to + 2047)
        if divisor == 1 {
            (roundf(offset)  * V_PER_OCTAVE) as i16 // whole volt (ie. octave) steps
        } else {
            // Continuous offset, quantized by divisor
            roundf(offset * V_PER_OCTAVE) as i16 / divisor
        }       
    }

    let faders_fut = async {
        let mut latch = app.make_latch(fader.get_value());
        loop {
            fader.wait_for_change().await;

            let latch_layer = glob_latch_layer.get();

            let target_value = match latch_layer {
                LatchLayer::Main => storage.query(|s| s.offset_voltage_saved),
                LatchLayer::Alt => storage.query(|s| s.offset_divisor_saved),
                LatchLayer::Third => 0,
            };

            if let Some(new_value) = latch.update(fader.get_value(), latch_layer, target_value) {
                match latch_layer {
                    LatchLayer::Main => {
                        storage.modify_and_save(|s| s.offset_voltage_saved = new_value);
                    }
                    LatchLayer::Alt => {
                        storage.modify_and_save(|s| s.offset_divisor_saved = new_value);
                    }
                    LatchLayer::Third => ()
                }
            }
        };
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

    let long_press_fut = async {
        //long press

        loop {
            let (_, is_shift_pressed) = buttons.wait_for_any_long_press().await;

            if !is_shift_pressed {
                // Toggle offset on/off
                storage.modify_and_save(|s| {
                    s.offset_enabled_saved = !s.offset_enabled_saved;
                    s.offset_enabled_saved
                });
            } 
        }
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

    join5(main_fut, faders_fut, btn_fut, long_press_fut, scene_handler).await;

}