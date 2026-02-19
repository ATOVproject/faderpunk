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

use crate::fp_grids_lib::resources::{
    DRUM_MAP, K_NUM_PARTS, K_NUM_STEPS_PER_PATTERN, LUT_RES_EUCLIDEAN, LUT_RES_EUCLIDEAN_SIZE,
};
use crate::fp_grids_lib::utils::{u8_mix, u8_u8_mul_shift8, Random};

/*
* Terminology:
*
* "Part":        "Voices" of generated sequence (e.g. BD/SD/HH for Drum mode)
* "Beat":        (e.g., quarter notes based on sequence_step).
* "Pulses":      Length of each gate (in 24ppqn, if not in gate mode)
* "Step":        Sequence step 0 - 31
*/

const K_PULSE_DURATION: u8 = 8; // 8 ticks of the main 24 ppqn clock

#[derive(Debug, Clone, Copy)]
pub enum PatternModeSettings {
    Drums { x: u8, y: u8, randomness: u8 },
    Euclidean { chaos_amount: u8 },
}

#[derive(Debug, Clone, Copy)]
pub struct PatternGeneratorSettings {
    pub options: PatternModeSettings,
    pub density: [u8; K_NUM_PARTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Ordinalize)]
pub enum OutputMode {
    OutputModeEuclidean,
    OutputModeDrums,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub output_mode: OutputMode,
    pub gate_mode: bool, // true for gate mode, false for trigger mode
}

#[allow(dead_code)]
pub enum OutputBits {
    OutputBitTrig1,
    OutputBitTrig2,
    OutputBitTrig3,
    OutputBitAccent,
    OutputBitClock, // May not be used directly by Disting NT plugin outputs
    OutputBitReset, // May not be used directly
}

impl OutputBits {
    pub fn to_bitmask(&self) -> u8 {
        match self {
            OutputBits::OutputBitTrig1 => 1,
            OutputBits::OutputBitTrig2 => 2,
            OutputBits::OutputBitTrig3 => 4,
            OutputBits::OutputBitAccent => 8,
            OutputBits::OutputBitClock => 16,
            OutputBits::OutputBitReset => 32,
        }
    }
}

// Core Grids pattern generator state and logic
#[derive(Debug, Clone, Copy)]
pub struct PatternGenerator {
    pub settings_: [PatternGeneratorSettings; 2], // Index 0 for Euclidean, 1 for Drums
    pub options_: Options,

    chaos_globally_enabled_: bool, // Master switch for chaos effects

    // Internal state variables
    // Commented out, unused in fp-grids
    // output_buffer: [u8; (K_NUM_STEPS_PER_PATTERN as usize) >> 3], // Stores the full pattern bitmask (not directly used for real-time state_)
    // pulse_counter: [u8; K_NUM_PARTS],                       // Tracks individual part pulse counts (if ever needed)
    // pulse_duration: [u8; K_NUM_PARTS],                      // Individual pulse durations (if ever needed, currently global via state_)
    current_euclidean_length: [u8; K_NUM_PARTS], // Active length for each Euclidean part
    fill: [u8; K_NUM_PARTS], // Calculated number of active steps for Euclidean parts, based on density
    // step_counter: [u8; K_NUM_PARTS],                        // Generic step counter per part, used for Euclidean perturbation in original code
    part_perturbation: [u8; K_NUM_PARTS], // Randomness value applied per part in Drum mode
    euclidean_step: [u8; K_NUM_PARTS], // Current step for each Euclidean generator (0 to length-1)

    state_: u8, // Holds the current trigger/accent state for the current tick for all parts + accent.
    step_: u8,  // Current step in the main 32-step sequence (0-31), synonymous with sequence_step_

    // Clock and timing related
    // beat_counter_: u8,                        // Counts beats (e.g., quarter notes based on sequence_step_).
    sequence_step_: u8, // Current step in the sequence (0-31), drives pattern evaluation.
    first_beat_: bool,  // True if current step is the first beat of the pattern.
    beat_: bool,        // True if current step is on a beat (typically quarter note).

    // State variables
    pulse_: u8,
    pulse_duration_counter_: u16,

    // Random number generator
    random: Random,
}

// Public API
impl PatternGenerator {
    /// Initializes with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns step in current sequence 0 - 31
    pub fn get_step(&self) -> u8 {
        self.step_
    }
    pub fn is_gate_mode_active(&self) -> bool {
        self.options_.gate_mode
    }
    pub fn get_current_output_mode(&self) -> OutputMode {
        self.options_.output_mode
    }
    pub fn set_output_mode(&mut self, mode: OutputMode) {
        self.options_.output_mode = mode;
    }
    pub fn set_gate_mode(&mut self, active: bool) {
        self.options_.gate_mode = active;
    }
    pub fn set_global_chaos(&mut self, enabled: bool) {
        self.chaos_globally_enabled_ = enabled;
    }
    /// Provides the current trigger state for all parts (and accent).
    /// Bit 0: Part 1 (BD/EUC1), Bit 1: Part 2 (SD/EUC2), Bit 2: Part 3 (HH/EUC3)
    /// Bit 3: Accent (in Drum mode or chaotic Euclidean)
    pub fn get_trigger_state(&self) -> u8 {
        self.state_
    }
    pub fn is_on_first_beat(&self) -> bool {
        self.first_beat_
    }
    pub fn is_on_beat(&self) -> bool {
        self.beat_
    }
    /// Sets internal random number generator seed
    pub fn set_seed(&mut self, seed: u16) {
        self.random.seed(seed);
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
        self.evaluate();
    }

    pub fn retrigger(&mut self) {
        // Re-evaluate the current step without advancing time
        self.evaluate();
    }

    /// Called on each tick of external clock (e.g., 24 ppqn)
    /// (removed original code related to internal clock)
    pub fn tick(&mut self, external_clock_tick: bool) {
        if !external_clock_tick {
            // Only process if there's an actual external clock tick
            return;
        }

        // Direct clocking: each external tick advances main sequence and all Euclidean parts
        self.sequence_step_ = (self.sequence_step_ + 1) % K_NUM_STEPS_PER_PATTERN;
        self.step_ = self.sequence_step_;

        for part in 0..K_NUM_PARTS {
            self.euclidean_step[part] =
                (self.euclidean_step[part] + 1) % self.current_euclidean_length[part];
        }

        self.first_beat_ = self.sequence_step_ == 0;
        let mut steps_per_beat = K_NUM_STEPS_PER_PATTERN / 4;
        if steps_per_beat == 0 {
            steps_per_beat = 1;
        } // Avoid division by zero for short patterns
        self.beat_ = self.sequence_step_.is_multiple_of(steps_per_beat);

        self.evaluate(); // Evaluate patterns based on the new step

        self.increment_pulse_counter(); // Handle pulse durations on every external tick, regardless of main step advancement
    }

    /// Set sequence length 1 - 32 steps
    pub fn set_length(&mut self, channel: usize, length: u8) {
        if channel > K_NUM_PARTS {
            return;
        }
        // Clamp length to 1 - 32
        self.current_euclidean_length[channel] = length.clamp(1, 32);
    }

    // fill_param_value is 0-255 from app
    pub fn set_fill(&mut self, channel: usize, fill_param_value: u8) {
        if channel > K_NUM_PARTS {
            return;
        }

        // Store the raw 0-255 parameter value in settings.density for Euclidean mode;
        // this value is then scaled for LUT lookup in EvaluateEuclidean.
        self.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density[channel] =
            fill_param_value;

        let current_length = self.current_euclidean_length[channel];
        if current_length == 0 {
            // Should be caught by set_length clamping, but defensive
            self.fill[channel] = 0;
            return;
        }

        // Scale fill_param_value (0-255) to the number of active steps (0 to current_length).
        // This fill_[channel] is mostly for potential display or alternative logic, not directly for LUT.
        let mut active_steps: u8 =
            ((fill_param_value as u16 * current_length as u16 + 127) / 255) as u8; // Add 127 for rounding

        if active_steps > current_length {
            active_steps = current_length; // Clamp active_steps to current length
        }
        if fill_param_value > 0 && active_steps == 0 && current_length > 0 {
            active_steps = 1; // Ensure at least one step if fill > 0
        }
        if fill_param_value == 255 {
            active_steps = current_length; // max fill means all steps active
        }

        self.fill[channel] = active_steps;
    }
}

impl Default for PatternGenerator {
    fn default() -> Self {
        Self {
            // output_buffer: [0; (K_NUM_STEPS_PER_PATTERN as usize) >> 3],
            // pulse_counter: [0; K_NUM_PARTS],
            // pulse_duration: [0; K_NUM_PARTS],
            // step_counter: [0; K_NUM_PARTS],
            part_perturbation: [0; K_NUM_PARTS],
            euclidean_step: [0; K_NUM_PARTS],
            options_: Options {
                output_mode: OutputMode::OutputModeDrums,
                gate_mode: false,
            },
            chaos_globally_enabled_: false,
            state_: 0,
            step_: 0, // Ensure step_ (if different from sequence_step_) is also init
            // beat_counter_: 0,
            sequence_step_: 0,
            first_beat_: true, // Initialize these as well
            beat_: true,
            pulse_: 0,
            pulse_duration_counter_: 0,
            settings_: [
                // Default settings for Euclidean mode
                PatternGeneratorSettings {
                    options: PatternModeSettings::Euclidean {
                        chaos_amount: 0, // No chaos initially
                    },
                    density: [128, 128, 128], // Density 128/255 maps to ~8 steps for a 16-step length
                },
                // Default settings for Drum Mode
                PatternGeneratorSettings {
                    options: PatternModeSettings::Drums {
                        x: 128,        // Center of map
                        y: 128,        // Center of map
                        randomness: 0, // No chaos initially
                    },
                    density: [255, 255, 255], // Full density BD/SD/HH by default
                },
            ],
            current_euclidean_length: [16; K_NUM_PARTS], // Default to 16 steps for all parts in Euclidean mode
            fill: [8; K_NUM_PARTS], // Default fill: 8 steps (50% for a 16-step length)
            random: Random::default(),
        }
    }
}
// Private / internal functions
impl PatternGenerator {
    fn increment_pulse_counter(&mut self) {
        self.pulse_duration_counter_ += 1;
        if self.pulse_duration_counter_ >= K_PULSE_DURATION as u16 && !self.options_.gate_mode {
            self.state_ = 0;
        }
    }

    /// Reads the drum map, interpolating between 4 points in the map.
    /// x and y are 0-255 coordinates for map interpolation.
    fn read_drum_map(&self, step: u8, instrument: u8, x: u8, y: u8) -> u8 {
        let i = x >> 6; // Determines a 2x2 cell in the 5x5 map based on X (quantized to 0-3 for cell index)
        let j = y >> 6; // Determines a 2x2 cell in the 5x5 map based on Y (quantized to 0-3 for cell index)

        // Ensure i and j are within bounds for a 5x5 map access (max index 3 for base of 2x2 cell)
        // DrumMapAccess::drum_map_ptr is 5x5; accessing [i+1] or [j+1] requires i,j <= 3.
        let i_idx = i.min(3) as usize;
        let j_idx = j.min(3) as usize;

        let a_map = DRUM_MAP[j_idx][i_idx]; // Top-left node in the interpolation cell
        let b_map = DRUM_MAP[j_idx][i_idx + 1]; // Top-right node
        let c_map = DRUM_MAP[j_idx + 1][i_idx]; // Bottom-left node
        let d_map = DRUM_MAP[j_idx + 1][i_idx + 1]; // Bottom-right node

        let offset = ((instrument as usize) * K_NUM_STEPS_PER_PATTERN as usize) + step as usize; // kStepsPerPattern is typically 32

        let a = a_map[offset];
        let b = b_map[offset];
        let c = c_map[offset];
        let d = d_map[offset];

        // Interpolation weights are x % 64 and y % 64, scaled to 0-255 (by << 2)
        let x_weight = (x % 64) << 2;
        let y_weight = (y % 64) << 2;

        u8_mix(
            u8_mix(a, b, x_weight), // Interpolate horizontally between a and b
            u8_mix(c, d, x_weight), // Interpolate horizontally between c and d
            y_weight,               // Interpolate vertically between the two horizontal results
        )
    }

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

    // Evaluates the drum pattern for the current step, applying perturbation based on settings and randomness.
    // Updates the self.state_ with the trigger/accent information for the current step.
    fn evaluate_drums(&mut self) {
        if self.step_ == 0 && self.pulse_ == 0 {
            // Generate perturbation only once at the very start of the 32-step sequence (when pulse_ is also 0)
            let mut randomness =
                match self.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options {
                    PatternModeSettings::Drums { randomness, .. } => randomness,
                    _ => 0, // Default to 0 if not in Drum mode, though this should never happen
                };
            randomness >>= 2; // Scale randomness for perturbation amount
            for part in 0..K_NUM_PARTS {
                self.part_perturbation[part] = u8_u8_mul_shift8(self.random.get_byte(), randomness);
            }
        }

        let current_step_in_pattern = self.step_;
        let mut new_state_for_tick = 0u8; // Accumulates trigger and accent bits for the current tick

        let (x, y) = match self.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options {
            PatternModeSettings::Drums { x, y, .. } => (x, y),
            _ => (0, 0), // Default to 0 if not in Drum mode, though this should never happen
        };
        let mut density_thresholds = [0u8; K_NUM_PARTS];

        for (part, density) in self.settings_[OutputMode::OutputModeDrums.ordinal() as usize]
            .density
            .iter()
            .enumerate()
            .take(K_NUM_PARTS)
        {
            density_thresholds[part] = *density;
        }

        let mut accent_bits_for_parts: u8 = 0; // Accumulates trigger and accent bits for the current tick
        for (part, threshold) in density_thresholds
            .iter()
            .enumerate()
            .by_ref()
            .take(K_NUM_PARTS)
        {
            let mut level: u8 = self.read_drum_map(current_step_in_pattern, part as u8, x, y);
            if level < 255 - self.part_perturbation[part] {
                level += self.part_perturbation[part];
            } else {
                level = 255;
            }

            if level > *threshold {
                if level > 192 {
                    // Threshold for accent
                    accent_bits_for_parts |= 1 << part; // Mark part 'part' (0,1,2) as having an accent
                }
                new_state_for_tick |= 1 << part; // Set trigger bit for part 'part' (maps to OUTPUT_BIT_TRIG_1/2/3)
            }
        }

        // Handle the ACCENT output bit (bit 3 / OUTPUT_BIT_ACCENT)
        // In this port, accent is triggered if any part has an accent.
        // Original Grids had more complex accent/common bit logic related to output_clock option.
        if accent_bits_for_parts != 0 {
            // If any part has an accent
            new_state_for_tick |= OutputBits::OutputBitAccent.to_bitmask();
        }

        // The original Grids' options_.output_clock logic for setting OUTPUT_BIT_RESET
        // and modifying accent_bits based on clock/bar information has been removed
        // to simplify for the plugin context. This port focuses on core triggers + one combined accent.
        self.state_ = new_state_for_tick; // Update the main trigger/accent state for the current tick
    }

    fn evaluate_euclidean(&mut self) {
        self.state_ = 0; // Clear previous state
        for part in 0..K_NUM_PARTS {
            let length = self.current_euclidean_length[part];

            if length == 0 {
                continue;
            }

            let fill_param_value =
                self.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density[part]; // 0 - 16 from app parameter

            // Revised logic for density_for_lut:
            // It should represent the desired number of fills/events.
            // Clamp to current length and max LUT density index (31).
            let mut desired_fills = fill_param_value;
            if desired_fills > length {
                desired_fills = length;
            }
            if desired_fills > 31 {
                desired_fills = 31; // Assuming LUT density part is 0-31
            }
            let density_for_lut = desired_fills;

            let mut address: u16 = (length as u16 - 1) * 32 + density_for_lut as u16;
            if address >= LUT_RES_EUCLIDEAN_SIZE as u16 {
                address = 0; // Should not happen with valid length / density
            }

            let pattern_bits: u32 = LUT_RES_EUCLIDEAN[address as usize];
            let current_step_in_part: u32 = self.euclidean_step[part] as u32; // Current step for this part (0 to length - 1)

            if (pattern_bits >> current_step_in_part) & 1 == 1 {
                self.state_ |= 1 << part; // Set trigger for part "part"
            }

            // Chaos perturbation for Euclidean mode - simplified from original Grids.
            // May need refinement for more nuanced behavior.
            let chaos =
                match self.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options {
                    PatternModeSettings::Euclidean { chaos_amount } => chaos_amount,
                    _ => 0,
                };
            if self.chaos_globally_enabled_
                && chaos > 0
                && self.random.get_word() % 256 < chaos as u16
            {
                // Randomly flip the state of this part for chaos effect
                if self.random.get_word().is_multiple_of(8) {
                    // Lower probability of flip for subtlety
                    self.state_ ^= 1 << part;
                }
                // Introduce randomn accents if chaos amount is high
                if chaos > 192 && self.random.get_word().is_multiple_of(16) {
                    self.state_ |= OutputBits::OutputBitAccent.to_bitmask();
                }
            }
        }
    }
}

//
// ************** UNIT TESTS ****************************
//
#[cfg(test)]
mod tests {
    use super::*;
    use env_logger::Env;

    fn init_logger() {
        let _ = env_logger::Builder::from_env(Env::default().default_filter_or("warn")).is_test(true).try_init();
    }

    #[test]
    fn test_initialization() {
        let mut generator = PatternGenerator::default();
        generator.reset(); // Ensure reset initializes state correctly
        assert_eq!(0, generator.step_);
        assert_eq!(0, generator.pulse_);
        assert_eq!(true, generator.first_beat_);
        assert_eq!(true, generator.beat_);
        assert_eq!(0, generator.state_);
        assert_eq!(0, generator.sequence_step_);
    }

    #[test]
    fn test_drum_map_interpolation() {
        let generator: PatternGenerator = PatternGenerator::default();
        // Test interpolation at the center of the first cell (0,0) to (63,63)
        let value = generator.read_drum_map(0, 0, 32, 32);
        // The expected value would depend on the contents of DRUM_MAP; this is just a placeholder assertion
        assert_eq!(value, 217);
    }

    #[test]
    fn test_evaluate_drums() {
        init_logger(); // Logs will now be visible

        let mut generator: PatternGenerator = PatternGenerator::default();
        generator.set_seed(0xFFF1);
        generator.options_.output_mode = OutputMode::OutputModeDrums;
        generator.options_.gate_mode = true;
        generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options =
            PatternModeSettings::Drums { x: 0, y: 0, randomness: 0 };
        generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].density =
            [31; K_NUM_PARTS];

        generator.evaluate();
        assert_eq!(0, generator.get_step());
        assert_eq!(13, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(1, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(2, generator.get_step());
        assert_eq!(2, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(3, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(4, generator.get_step());
        assert_eq!(6, generator.get_trigger_state());
    }

    #[test]
    fn test_evaluate_euclidean() {
        init_logger(); // Logs will now be visible

        let mut generator: PatternGenerator = PatternGenerator::default();
        generator.set_seed(0xFFF1);
        generator.options_.output_mode = OutputMode::OutputModeEuclidean;
        generator.options_.gate_mode = true;
        generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options =
            PatternModeSettings::Euclidean { chaos_amount: 0 };
        generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density =
            [31; K_NUM_PARTS];

        for part in 0..K_NUM_PARTS {
            let length = 8;
            generator.set_length(part, length);
            let fill_density_param = 31;
            generator.set_fill(part, fill_density_param);
        }

        generator.evaluate();
        assert_eq!(7, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(1, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(true);
        assert_eq!(2, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());
        generator.tick(true);
        assert_eq!(3, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());
        generator.tick(true);
        assert_eq!(4, generator.get_step());
        assert_eq!(7, generator.get_trigger_state());
    }
}
