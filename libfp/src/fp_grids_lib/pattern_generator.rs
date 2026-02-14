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

use enum_ordinalize::Ordinalize;

/*
* Terminology:
*
* "Part":        "Voices" of generated sequence (e.g. BD/SD/HH for Drum mode)
* "Beat":        (e.g., quarter notes based on sequence_step).
* "Pulses":      aka. 24 ppqn clock ticks
* "Step":        Sequence step 0 - 31
*/
use super::resources::{K_NUM_PARTS, K_NUM_STEPS_PER_PATTERN};

const K_PULSES_PER_STEP:u8 = 3; // 24 ppqn ; 8 steps per quarter note
const K_PULSE_DURATION:u8 = 8; // 8 ticks of the main 24 ppqn clock

#[derive(Debug, Clone, Copy)]
enum PatternModeSettings {
    Drums {x: u8, y: u8, randomness: u8},
    Euclidean {chaos_amount: u8 },
}

#[derive(Debug, Clone, Copy)]
struct PatternGeneratorSettings {
    options: PatternModeSettings,
    density: [u8; K_NUM_PARTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Ordinalize)]
pub enum OutputMode {
    OutputModeEuclidean,
    OutputModeDrums,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Options {
    output_mode: OutputMode,
    gate_mode: bool, // true for gate mode, false for trigger mode
}

// Core Grids pattern generator state and logic
#[derive(Debug, Clone, Copy)]
pub struct PatternGenerator {

    settings_: [PatternGeneratorSettings; 2],               // Index 0 for Euclidean, 1 for Drums
    options_: Options,

    // Internal state variables
    output_buffer: [u8; (K_NUM_STEPS_PER_PATTERN as usize) >> 3],          // Stores the full pattern bitmask (not directly used for real-time state_)
    pulse_counter: [u8; K_NUM_PARTS],                       // Tracks individual part pulse counts (if ever needed)
    pulse_duration: [u8; K_NUM_PARTS],                      // Individual pulse durations (if ever needed, currently global via state_)
    current_euclidean_length: [u8; K_NUM_PARTS],            // Active length for each Euclidean part
    fill: [u8; K_NUM_PARTS],                                // Calculated number of active steps for Euclidean parts, based on density
    step_counter: [u8; K_NUM_PARTS],                        // Generic step counter per part, used for Euclidean perturbation in original code
    part_perturbation: [u8; K_NUM_PARTS],                   // Randomness value applied per part in Drum mode
    euclidean_step: [u8; K_NUM_PARTS],                      // Current step for each Euclidean generator (0 to length-1)

    state_: u8,                                              // Holds the current trigger/accent state for the current tick for all parts + accent.
    step_: u8,                                               // Current step in the main 32-step sequence (0-31), synonymous with sequence_step_

    // Clock and timing related
    internal_clock_ticks_: u16,               // Counts sub-ticks for original Grids clocking mode.
    beat_counter_: u8,                        // Counts beats (e.g., quarter notes based on sequence_step_).
    sequence_step_: u8,                       // Current step in the sequence (0-31), drives pattern evaluation.
    swing_applied_: bool,                     // Tracks if swing has been applied in the current sub-step (more relevant to original complex swing).
    first_beat_: bool,                        // True if current step is the first beat of the pattern.
    beat_: bool,                              // True if current step is on a beat (typically quarter note).

    // State variables
    pulse_: u8,
    pulse_duration_counter_: u16,

}

// Public API
impl PatternGenerator {

    /// Initializes with default settings
    pub fn new() -> Self {
       Self::default()
    }

    /// Reset a running generator
    pub fn reset(&mut self) {
        self.step_ = 0;
        self.pulse_ = 0;
        self.euclidean_step = [0; K_NUM_PARTS];
        self.part_perturbation = [0; K_NUM_PARTS];
        self.first_beat_ = true;
        self.beat_ = true;
        self.state_ = 0;
        self.pulse_duration_counter_ = 0;
        self.internal_clock_ticks_ = 0;
        self.evaluate();
    }



}

impl Default for PatternGenerator {
    fn default() -> Self {
        Self {
            output_buffer: [0; (K_NUM_STEPS_PER_PATTERN as usize) >> 3],
            pulse_counter: [0; K_NUM_PARTS],
            pulse_duration: [0; K_NUM_PARTS],
            step_counter: [0; K_NUM_PARTS],
            part_perturbation: [0; K_NUM_PARTS],
            euclidean_step: [0; K_NUM_PARTS],
            options_: Options {
                output_mode: OutputMode::OutputModeDrums,
                gate_mode: false,
            },
            state_: 0,
            step_: 0, // Ensure step_ (if different from sequence_step_) is also init
            internal_clock_ticks_: 0,
            beat_counter_: 0,
            sequence_step_: 0,
            swing_applied_: false,
            first_beat_: true, // Initialize these as well
            beat_: true,
            pulse_: 0,
            pulse_duration_counter_: 0,
            settings_: [
                // Default settings for Euclidean mode
                PatternGeneratorSettings {
                    options: PatternModeSettings::Euclidean {
                        chaos_amount: 0,          // No chaos initially
                    },
                    density: [128, 128, 128],     // Density 128/255 maps to ~8 steps for a 16-step length
                },
                // Default settings for Drum Mode
                PatternGeneratorSettings {
                    options: PatternModeSettings::Drums {
                        x: 128,         // Center of map
                        y: 128,         // Center of map
                        randomness: 0   // No chaos initially
                    },
                    density: [255, 255, 255], // Full density BD/SD/HH by default
                }
            ],
            current_euclidean_length: [16; K_NUM_PARTS],  // Default to 16 steps for all parts in Euclidean mode
            fill: [8; K_NUM_PARTS],                       // Default fill: 8 steps (50% for a 16-step length)
        }
    }
}
// Private / internal functions
impl PatternGenerator {
    fn evaluate(&mut self) {
        self.pulse_duration_counter_ = 0; // Reset pulse timer for new evaluation cycle

        // The general random bit (0x80) from original Grids' Evaluate() that was ORed into state_
        // is not explicitly added here. Accent generation is now handled within EvaluateDrums/Euclidean.
        // A generic random trigger output could be added here if desired as a separate feature.
        match self.options_.output_mode {
            OutputMode::OutputModeDrums => self.evaluate_drums(),
            OutputMode::OutputModeEuclidean => self.evaluate_euclidean(),
        }

    }

    fn evaluate_drums(&mut self) {
        // Placeholder for Drum mode evaluation logic
    }

    fn evaluate_euclidean(&mut self) {
        // Placeholder for Euclidean mode evaluation logic
    }
}

//
// ************** UNIT TESTS ****************************
//
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization() {
        let mut generator = PatternGenerator::default();
        generator.reset(); // Ensure reset initializes state correctly
        assert_eq!(generator.step_, 0);
        assert_eq!(generator.pulse_, 0);
        assert_eq!(generator.first_beat_, true);
        assert_eq!(generator.beat_, true);
        assert_eq!(generator.state_, 0);
        assert_eq!(generator.internal_clock_ticks_, 0);
        assert_eq!(generator.beat_counter_, 0);
        assert_eq!(generator.sequence_step_, 0);
        assert_eq!(generator.swing_applied_, false);
    }

}




