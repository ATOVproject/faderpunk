// Copyright (C) 2012 Emilie Gillet.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.
//
// Based on the original Grids by Emilie Gillet.

//! # FP-Grids
//! 
//! A port of Emilie Gillet's renowned Mutable Instruments Grids topographic drum sequencer for the ATOV Faderpunk
//! 
//! Grids is described as a "topographic drum sequencer" - it generates a variety of drum patterns based on continuous interpolation through a "map" of patterns (Drum Mode) or using Euclidean algorithms (Euclidean Mode).
//! 
//! The original Eurorack module manual is [here](https://pichenettes.github.io/mutable-instruments-documentation/modules/grids/manual/)
//! 
//! ## Features
//! 
//! * **Two modes** - Switch between classic Drum map interpolation and Euclidean pattern generation.
//! * **Four-channel Faderpunk app** - three trigger outputs and an additional accent trigger output
//! * **Global Clock** - uses the global Faderpunk clock
//! * **Chaos** - Introduce controlled randomness to patterns (On/Off, with Amount control).
//! * **Scene Storage & Recall** - save dynamic state of generator and recall in Faderpunk scenes
//! * **MIDI Output** - MIDI Note per drum trigger and accent
//! * **Fader Memory** - Remembers mode-specific fader settings
//!
//! Trigger output signals are a fixed duration ~5ms +5V high, 0V low
//! 
//! ## Modes
//! 
//! ### 1. Drum Mode
//! 
//! Generates patterns by interpolating through a 2D map of pre-analyzed drum patterns.
//! 
//! * **Map X / Map Y:** Controls the position on the pattern map. Small changes typically result in related rhythmic variations.
//! * **Density 1 / Density 2 / Density 3:** Controls the event density (fill) for each of the three main trigger outputs.
//! * **Chaos Amount:** Controls the amount of randomness applied (when Chaos is enabled).
//! 
//! ### 2. Euclidean Mode
//! 
//! Generates classic Euclidean rhythms for each of the three main trigger outputs independently.
//!
//! * **Length 1 / Length 2 / Length 3:** Sets the total number of steps in the sequence for each output (1-16).
//! * **Fill 1 / Fill 2 / Fill 3:** Sets the number of triggers distributed as evenly as possible within the sequence length for each output (0-Length).
//! * **Chaos Amount:** Controls the amount of random step-skipping/triggering (when Chaos is enabled).
//! 
//! ## Hardware Mapping in Drum Mode
//! 
//! | Control | Function | + Shift | + Fn
//! |---------|----------|---------|------|
//! | Jack 1  | Kick Out | N/A     | N/A  | 
//! | Fader 1  | Density 1 | Map X  | Speed  |
//! | LED 1 Top | Gate output | Gate output | N/A
//! | LED 1 Bottom | Density 1 | Map X | Speed
//! | Fn 1    | Mute Trigger 1 | N/A | N/A |
//! | Jack 2  | Snare Out | N/A     | N/A  | 
//! | Fader 2  | Density 2 | Map Y  | N/A  |
//! | LED 2 Top | Gate output | Gate output | N/A
//! | LED 2 Bottom | Density 2 | Map Y | N/A
//! | Fn 2    | Mute Trigger 2 | N/A | N/A |
//! | Jack 3  | Hi-Hats Out | N/A     | N/A  | 
//! | Fader 3  | Density 3 | N/A  | N/A  |
//! | LED 3 Top | Gate output | Gate output | N/A
//! | LED 3 Bottom | Density 3 | Chaos | N/A
//! | Fn 3    | Mute Trigger 3 | N/A | N/A |
//! | Jack 4  | Accent Out | N/A     | N/A  | 
//! | Fader 4  | Chaos | N/A  | N/A  |
//! | LED 4 Top | Accent output | Accent output | N/A
//! | LED 4 Bottom | Clock pulse | N/A | N/A
//! | Fn 4    | Mute Accent | Chaos on/off | N/A |
//! 
//! ## Hardware Mapping in Euclidean
//! 
//! | Control | Function | + Shift | + Fn
//! |---------|----------|---------|------|
//! | Jack 1  | Trigger 1 Out | N/A     | N/A  | 
//! | Fader 1  | Length 1 | Fill 1  | N/A  |
//! | LED 1 Top | Gate output | Gate output | N/A
//! | LED 1 Bottom | Length 1 | Fill 2 | N/A
//! | Fn 1    | Mute Trigger 1 | N/A | N/A |
//! | Jack 2  | Trigger 2 Out | N/A     | N/A  | 
//! | Fader 2  | Length 2 | Fill 2  | N/A  |
//! | LED 2 Top | Gate output | Gate output | N/A
//! | LED 2 Bottom | Length 2 | Fill 2 | N/A
//! | Fn 2   | Mute Trigger 2 | N/A | N/A |
//! | Jack 3  | Trigger 3 Out | N/A     | N/A  | 
//! | Fader 3  | Length 3 | Fill 3  | N/A  |
//! | LED 3 Top | Gate output | Gate output | N/A
//! | LED 3 Bottom | Length 3 | Fill 3 | N/A
//! | Fn 3    | Mute Trigger 3 | N/A | N/A |
//! | Jack 4  | Accent Out | N/A     | N/A  | 
//! | Fader 4  | Chaos | N/A  | N/A  |
//! | LED 4 Top | Accent output | Accent output | N/A
//! | LED 4 Bottom | Chaos | N/A | N/A
//! | Fn 4    | Mute Accent | Chaos on/off | N/A |
//! 
//! ## App Configuration
//! 
//! * Mode (Drum or Euclidean)
//! * MIDI Channel
//! * MIDI NOTE 1
//! * MIDI NOTE 2
//! * MIDI NOTE 3
//! * MIDI Velocity
//! * MIDI Velocity (Accent)
//! * GATE %
//! * Color
//! * Midi device outs
//! 
//! ## Acknowledgements
//! 
//! * Original Concept & Code: Emilie Gillet (Mutable Instruments). The original Eurorack module source code can be found [here](https://github.com/pichenettes/eurorack/tree/master/grids).
//! * Faderpunk Port: Richard Smith (GitHub: rjsmith)
//! * Special acknowledgement: [Disting NT Port](https://github.com/thorinside/nt_grids/tree/main) by Neal Sanche (GitHub: Thorinside)
//! 
//! TODO:
//! * Speed 

use embassy_futures::{
    join::{join4}, select::{select, select3}
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use heapless::Vec;
use enum_ordinalize::Ordinalize;

use libfp::{
    APP_MAX_PARAMS, AppIcon, Brightness, ClockDivision, Color, Config, MidiChannel, MidiNote, MidiOut, 
    Param, Value, ext::FromValue, fp_grids_lib::{K_NUM_PARTS, OutputBits, OutputMode, PatternGenerator, PatternModeSettings},
    latch::LatchLayer, utils::{scale_bits_12_8}
};

use serde::{Deserialize, Serialize};

use crate::app::{App, AppParams, AppStorage, ClockEvent, Global, Led, ManagedStorage, ParamStore, SceneEvent };

pub const CHANNELS: usize = 4;
pub const PARAMS: usize = 10;

// App configuration visible to the configurator
pub static CONFIG: Config<PARAMS> = Config::new(
    "FP Grids",
    "Topographic drum sequencer port of Mutable Instruments Grids, synced to internal clock",
    Color::SkyBlue,
    AppIcon::Euclid,
)
.add_param(Param::Enum { name: "Mode", variants: &["Euclidean", "Drums"] })
.add_param(Param::MidiChannel {
    name: "MIDI Channel",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 1",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 2",
})
.add_param(Param::MidiNote {
    name: "MIDI Note 3",
})
.add_param(Param::i32 { name: "MIDI Velocity", min: 1, max: 127 
})
.add_param(Param::i32 { name: "MIDI Accent Vel", min: 1, max: 127
})
.add_param(Param::i32 {
    name: "GATE %",
    min: 1,
    max: 100,
})
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
.add_param(Param::MidiOut);

pub struct Params {
    midi_channel: MidiChannel,
    midi_out: MidiOut,
    note1: MidiNote,
    note2: MidiNote,
    note3: MidiNote,
    velocity: i32,
    accent: i32,
    gatel: i32,
    color: Color,
    mode: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            midi_channel: MidiChannel::default(),
            midi_out: MidiOut::default(),
            note1: MidiNote::from(36),
            note2: MidiNote::from(38),
            note3: MidiNote::from(42),
            velocity: 100,
            accent: 127,
            gatel: 50,
            color: Color::Orange,
            mode: OutputMode::OutputModeDrums.ordinal() as usize, 
        }
    }
}

impl AppParams for Params {
    fn from_values(values: &[Value]) -> Option<Self> {
        if values.len() < PARAMS {
            return None;
        }
        Some(Self {
            mode: usize::from_value(values[0]),
            midi_channel: MidiChannel::from_value(values[1]),
            note1: MidiNote::from_value(values[2]),
            note2: MidiNote::from_value(values[3]),
            note3: MidiNote::from_value(values[4]),
            velocity: i32::from_value(values[5]),
            accent: i32::from_value(values[6]),
            gatel: i32::from_value(values[7]),
            color: Color::from_value(values[8]),
            midi_out: MidiOut::from_value(values[9]),
        })
    }

    fn to_values(&self) -> Vec<Value, APP_MAX_PARAMS> {
        let mut vec = Vec::new();
        vec.push(self.mode.into()).unwrap();
        vec.push(self.midi_channel.into()).unwrap();
        vec.push(self.note1.into()).unwrap();
        vec.push(self.note2.into()).unwrap();
        vec.push(self.note3.into()).unwrap();
        vec.push(self.velocity.into()).unwrap();
        vec.push(self.accent.into()).unwrap();
        vec.push(self.gatel.into()).unwrap();
        vec.push(self.color.into()).unwrap();
        vec.push(self.midi_out.into()).unwrap();
        vec
    }
}

#[derive(Serialize, Deserialize)]
pub struct Storage {
    fader_saved: [u16; K_NUM_PARTS],
    shift_fader_saved: [u16; K_NUM_PARTS],
    chaos_enabled_saved: bool, 
    chaos_saved: u16,           // 0 - 255 scaled range
    div_saved: u16,             // 0 - 4095 range, maps to index into 'resolution' clock div array (same as euclid.rs)
    mute_saved: [bool; K_NUM_PARTS + 1],      // 3 triggers + accent
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            fader_saved: [2047; K_NUM_PARTS],
            shift_fader_saved: [2047; K_NUM_PARTS],
            chaos_enabled_saved: false,
            chaos_saved: 0,
            div_saved: 3000,
            mute_saved: [false; K_NUM_PARTS + 1],
        }
    }
}
impl AppStorage for Storage {}

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

pub async fn run(
    app: &App<CHANNELS>,
    params: &ParamStore<Params>,
    storage: &ManagedStorage<Storage>,
) {
    let die = app.use_die();
    let faders = app.use_faders();
    let buttons = app.use_buttons();
    let leds = app.use_leds();

    let (midi_out, midi_channel, gatel, note1, note2, note3, velocityi32, accent_velocityi32, led_color) = params.query(|p| {
        (
            p.midi_out,
            p.midi_channel,
            p.gatel,
            p.note1,
            p.note2,
            p.note3,
            p.velocity,
            p.accent,
            p.color,
        )
    });
    let midi_velocity = ((velocityi32.abs().clamp(1, 127) as u32 * 4095) / 127) as u16;
    let accent_velocity = ((accent_velocityi32.abs().clamp(1, 127) as u32 * 4095) / 127) as u16;

    let output_mode: OutputMode = match OutputMode::from_ordinal(params.query(|p| p.mode) as i8) {
        None => OutputMode::OutputModeDrums,
        Some(mode) => mode
    };

    let midi = app.use_midi_output(midi_out, midi_channel);    
    let notes = [note1, note2, note3];
    let jack = [
        app.make_gate_jack(0, 0).await,
        app.make_gate_jack(1, 0).await,
        app.make_gate_jack(2, 0).await,
        app.make_gate_jack(3, 0).await,
    ];
    let resolution = [384u32, 192, 96, 48, 24, 16, 12, 8, 6, 4, 3, 2];
    let div_glob = app.make_global(6); // = 1/16th note
    let glob_latch_layer = app.make_global(LatchLayer::Main);
    // use globs to track pattern generator parameter values, transformed from fader values
    let drums_density_glob = app.make_global([127u8; K_NUM_PARTS]); // 0 - 255 fill density
    let drums_map_x_glob = app.make_global(127u8); // 0 - 255 Map X
    let drums_map_y_glob = app.make_global(127u8); // 0 - 255 Map Y
    let euclidean_length_glob = app.make_global([16u8; K_NUM_PARTS]); // 1 - 32 steps
    let euclidean_fill_glob = app.make_global([64u8; K_NUM_PARTS]); // 0 - 255 fill
    let chaos_glob = app.make_global(0u8);

    // TODO : Initialise pattern generator globs from storage


    let main_loop = async {
        let mut clock = app.use_clock();
        let mut clkn: u32 = 0;
        let mut note_on = [false; K_NUM_PARTS];
        let mut accent_on = false;

        let mut generator = PatternGenerator::default();
        generator.set_seed(die.roll());
        generator.set_output_mode(output_mode);
        update_generator_from_parameters(&mut generator, &GeneratorUpdateContext {
            drums_density_glob: &drums_density_glob,
            drums_map_x_glob: &drums_map_x_glob,
            drums_map_y_glob: &drums_map_y_glob,
            euclidean_length_glob: &euclidean_length_glob,
            euclidean_fill_glob: &euclidean_fill_glob,
            chaos_glob: &chaos_glob
        });
        generator.reset();

        loop {
            match clock.wait_for_event(ClockDivision::_1).await {

                ClockEvent::Reset => {
                    clkn = 0;
                    generator.reset();
                    generator.set_seed(die.roll());
                    midi.send_note_off(note1).await;
                    midi.send_note_off(note2).await;
                    midi.send_note_off(note3).await;
                    note_on = [false; K_NUM_PARTS];
                    accent_on = false;
                    jack[0].set_low().await;
                }
                ClockEvent::Tick => {
                    let muted = storage.query(|s| s.mute_saved);
                    let div = div_glob.get();

                    // If we have reached the next sequence step
                    if clkn.is_multiple_of(div) {

                        // Get generator state and handle individual triggers
                        let state = generator.get_trigger_state();
                        let is_accent = state & (1 << 3) > 0;
                        let velocity_ = if is_accent {
                            midi_velocity
                        } else {
                            accent_velocity
                        };

                        for (part, note) in notes.iter().enumerate().take(K_NUM_PARTS) {
                            if state & (1 << part) > 0 {
                                // Trigger fired
                                jack[part].set_high().await;
                                // Send Note Off first if re-triggering
                                if note_on[part] {
                                    midi.send_note_off(*note).await;
                                }
                                note_on[part] = true;
                                midi.send_note_on(*note, velocity_).await;
                                leds.set(part, Led::Top, led_color, if is_accent {Brightness::High} else {Brightness::Mid});
                            }
                        }
     
                        if is_accent {
                            // Accent fired
                            jack[3].set_high().await;
                            accent_on = true;
                            leds.set(3, Led::Top, led_color, Brightness::High);
                        }

                        // Update generator with parameter changes
                        update_generator_from_parameters(&mut generator, &GeneratorUpdateContext {
                            drums_density_glob: &drums_density_glob,
                            drums_map_x_glob: &drums_map_x_glob,
                            drums_map_y_glob: &drums_map_y_glob,
                            euclidean_length_glob: &euclidean_length_glob,
                            euclidean_fill_glob: &euclidean_fill_glob,
                            chaos_glob: &chaos_glob
                        });

                        // Finally, progress pattern ready for evaluation on next clocked sequence step
                        generator.tick(true);
                    }

                    // If reached end of gate length between sequence steps
                    if clkn % div == (div * gatel as u32 / 100).clamp(1, div - 1) {
                        for (part, note) in notes.iter().enumerate().take(K_NUM_PARTS) {
                            if note_on[part] {
                                midi.send_note_off(*note).await;
                                note_on[part] = false;
                                jack[part].set_low().await;
                            }
                            leds.unset(part, Led::Top);
                        }
                        // Accent jack
                        if accent_on {
                            accent_on = false;
                            jack[3].set_low().await;
                            leds.unset(3, Led::Top);
                        }
                    }

                    clkn += 1;
                }
                _ => {}
            }
        }
    };

    let fader_fut = async {
        let mut latch = [
            app.make_latch(faders.get_value_at(0)),
            app.make_latch(faders.get_value_at(1)),
            app.make_latch(faders.get_value_at(2)),
            app.make_latch(faders.get_value_at(3)),
        ];
        loop {
            let chan = faders.wait_for_any_change().await;
            let latch_layer = glob_latch_layer.get();
            match output_mode {
                OutputMode::OutputModeDrums => {
                    match chan {
                        0 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                                LatchLayer::Third => storage.query(|s| s.div_saved),
                            };
                            if let Some(new_value) =
                                latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                            {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    },
                                    LatchLayer::Alt => {
                                        // Convert fader value 0 .. 4095 12 bit to Drums Map X 0 .. 255
                                        drums_map_x_glob.set(scale_bits_12_8(new_value));
                                        storage.modify_and_save(|s| s.shift_fader_saved[chan] = new_value);
                                    },
                                    LatchLayer::Third => {
                                        div_glob.set(resolution[new_value as usize / 345]);
                                        storage.modify_and_save(|s| s.div_saved = new_value);
                                    }
                                };
                            }
                        },
                        1 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                LatchLayer::Alt => storage.query(|s| s.shift_fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) =
                                latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                            {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    },
                                    LatchLayer::Alt => {
                                        // Convert fader value 0 .. 4095 12 bit to Drums Map X 0 .. 255
                                        drums_map_y_glob.set(scale_bits_12_8(new_value));
                                        storage.modify_and_save(|s| s.shift_fader_saved[chan] = new_value);
                                    },
                                    _ => {}
                                };
                            }
                           
                        },
                        2 => {
                            let target_value = match latch_layer {
                                LatchLayer::Main => storage.query(|s| s.fader_saved[chan]),
                                _ => 0,
                            };
                            if let Some(new_value) =
                                latch[chan].update(faders.get_value_at(chan), latch_layer, target_value)
                            {
                                match latch_layer {
                                    LatchLayer::Main => {
                                        // Convert fader value 0 .. 4095 12-bit to Drums density 0 .. 255 8 - bit
                                        let mut drums_density = drums_density_glob.get();
                                        drums_density[chan] = scale_bits_12_8(new_value);
                                        drums_density_glob.set(drums_density);
                                        storage.modify_and_save(|s| s.fader_saved[chan] = new_value);
                                    },
                                    _ => {}
                                };
                            }

                        },
                        _ => {},

                    };
                },
                OutputMode::OutputModeEuclidean => {

                }
            };
            



           
        }
    };

   let shift_fut = async {
        loop {
            // latching on pressing and depressing shift
            app.delay_millis(1).await;

            let latch_active_layer = if buttons.is_shift_pressed() && !buttons.is_button_pressed(0)
            {
                LatchLayer::Alt
            } else if !buttons.is_shift_pressed() && buttons.is_button_pressed(0) {
                LatchLayer::Third
            } else {
                LatchLayer::Main
            };
            glob_latch_layer.set(latch_active_layer);
        }
    };
    let scene_handler = async {
        loop {
            match app.wait_for_scene_event().await {
                SceneEvent::LoadScene(scene) => {
                    storage.load_from_scene(scene).await;

                    let division = storage.query(|s| s.div_saved);
                    div_glob.set(resolution[division as usize / 345]);
                }

                SceneEvent::SaveScene(scene) => {
                    storage.save_to_scene(scene).await;
                }
            }
        }
    };

    join4(main_loop, fader_fut, shift_fut, scene_handler).await;


}

struct GeneratorUpdateContext<'a> {
    drums_density_glob: &'a Global<[u8; K_NUM_PARTS]>,
    drums_map_x_glob: &'a Global<u8>,
    drums_map_y_glob: &'a Global<u8>,
    euclidean_length_glob: &'a Global<[u8; K_NUM_PARTS]>,
    euclidean_fill_glob: &'a Global<[u8; K_NUM_PARTS]>,
    chaos_glob: &'a Global<u8>
}

/// Update a PatternGenerator options from the app instance's managed parameters
fn update_generator_from_parameters(generator: &mut PatternGenerator, settings: &GeneratorUpdateContext) {
    generator.set_gate_mode(true);
    generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options =
            PatternModeSettings::Drums { x: settings.drums_map_x_glob.get(), y: settings.drums_map_y_glob.get(), randomness: settings.chaos_glob.get() };
    generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].density =
            settings.drums_density_glob.get();     
    generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options =
            PatternModeSettings::Euclidean { chaos_amount: settings.chaos_glob.get() };
    generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density =
            settings.euclidean_fill_glob.get();
    let length: [u8; K_NUM_PARTS] = settings.euclidean_length_glob.get();
    let fill: [u8; K_NUM_PARTS] = settings.euclidean_fill_glob.get();
    for part in 0..K_NUM_PARTS {
        generator.set_length(part, length[part]);
        let fill_density_param = 31;
        generator.set_fill(part, fill[part]);
    }
}