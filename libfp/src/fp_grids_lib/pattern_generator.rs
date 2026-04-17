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
//
// DnB sequencer ported from https://github.com/thorinside/dnb_seq
//
use enum_ordinalize::Ordinalize;
use serde::{Deserialize, Serialize};

use crate::fp_grids_lib::resources::{DRUM_MAP, K_NUM_PARTS, K_NUM_STEPS_PER_PATTERN};
use crate::fp_grids_lib::utils::{u8_mix, u8_u8_mul_shift8, Random};
use crate::utils::euclidean_pattern;

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
    DnB { pattern: u8 },
}

#[derive(Debug, Clone, Copy)]
pub struct PatternGeneratorSettings {
    pub options: PatternModeSettings,
    /// In Drums & DnB mode, 0-255, in Euclidean 1-16
    pub density: [u8; K_NUM_PARTS],
}

#[derive(Debug, Clone, Copy, PartialEq, Ordinalize)]
pub enum OutputMode {
    OutputModeEuclidean,
    OutputModeDrums,
    OutputModeDnB,
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

pub const DNB_MAX_STEPS: usize = 32;
pub const DNB_NUM_PATTERNS: u8 = 12;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DnbDrumPattern {
    pub kick: [bool; DNB_MAX_STEPS],
    pub snare: [bool; DNB_MAX_STEPS],
    pub hihat: [bool; DNB_MAX_STEPS],
    pub has_ghost: bool,
    pub ghost_snare: [bool; DNB_MAX_STEPS],
    pub steps: u8, // Num steps in the pattern
}
impl Default for DnbDrumPattern {
    fn default() -> Self {
        Self {
            kick: [false; DNB_MAX_STEPS],
            snare: [false; DNB_MAX_STEPS],
            hihat: [false; DNB_MAX_STEPS],
            has_ghost: false,
            ghost_snare: [false; DNB_MAX_STEPS],
            steps: 16,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SequencerState {
    pub sequence_step: u8,
    pub euclidean_step: [u8; K_NUM_PARTS],
    pub pulse: u8,
    pub pulse_duration_counter: u16,
    pub current_dnb_pattern: DnbDrumPattern,
    pub base_dnb_pattern: DnbDrumPattern,
}
impl Default for SequencerState {
    fn default() -> Self {
        Self {
            sequence_step: 0,
            euclidean_step: [0; K_NUM_PARTS],
            pulse: 0,
            pulse_duration_counter: 0,
            base_dnb_pattern: DnbDrumPattern::default(),
            current_dnb_pattern: DnbDrumPattern::default(),
        }
    }
}

// Core Grids pattern generator state and logic
#[derive(Debug, Clone, Copy)]
pub struct PatternGenerator {
    pub settings_: [PatternGeneratorSettings; 3], // Index 0 for Euclidean, 1 for Drums, 2 for DnB
    pub options_: Options,

    chaos_globally_enabled_: bool, // Master switch for chaos effects

    // Internal state variables
    current_euclidean_length: [u8; K_NUM_PARTS], // Active length for each Euclidean part
    fill: [u8; K_NUM_PARTS], // Calculated number of active steps for Euclidean parts, based on density
    part_perturbation: [u8; K_NUM_PARTS], // Randomness value applied per part in Drum mode
    euclidean_step: [u8; K_NUM_PARTS], // Current step for each Euclidean generator (0 to length-1)
    euclidean_offset: [u8; K_NUM_PARTS], // Per-part Euclidean rotation offset (0 to length-1)

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

    // Easter Egg DnB pattern generator
    current_dnb_pattern: DnbDrumPattern, // Currently playing pattern
    base_dnb_pattern: DnbDrumPattern,    // The original, unmodified pattern
    queued_pattern_id: i8,               // -1 = no pattern change queued
    pattern_change_queued: bool,         // = true if pending pattern change queued for next bar
                                         // NB: Hi hat always trieggers when pattern is active (no automated muting)
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

    /// Returns internal generator seqauencer state to be stored in an app's ManagedStorage so it can be restored later after a re-spwan
    pub fn get_sequencer_state(&self) -> SequencerState {
        SequencerState {
            sequence_step: self.sequence_step_,
            euclidean_step: self.euclidean_step,
            pulse: self.pulse_,
            pulse_duration_counter: self.pulse_duration_counter_,
            base_dnb_pattern: self.base_dnb_pattern,
            current_dnb_pattern: self.current_dnb_pattern,
        }
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

    /// Queue up DnB pattern change on next bar (ie when first_beat_ = true)
    pub fn queue_dnb_pattern_change(&mut self, new_pattern: u8) {
        self.queued_pattern_id = new_pattern.clamp(0, DNB_NUM_PATTERNS - 1) as i8;
        self.pattern_change_queued = true;
    }

    /// Reset DnB pattern to default
    pub fn reset_dnb_pattern_to_base(&mut self) {
        self.current_dnb_pattern = self.base_dnb_pattern;
    }

    /// Required per-step 24ppqn clock division for current DnB pattern
    pub fn get_dnb_24ppqn_pattern_division(&self) -> u32 {
        match self.current_dnb_pattern.steps {
            16 | 32 => 6, // 1/16th notes, the only 32-step DnB pattern is supposed to be 2 bars long
            24 => 4,      // 1/16th note triplets
            _ => 6,
        }
    }

    /// Reset a running generator, but doesn't change pattern
    pub fn reset(&mut self) {
        self.step_ = 0;
        self.sequence_step_ = 0;
        self.pulse_ = 0;
        self.euclidean_step = [0; K_NUM_PARTS];
        self.part_perturbation = [0; K_NUM_PARTS];
        self.first_beat_ = true;
        self.beat_ = true;
        self.state_ = 0;
        self.pulse_duration_counter_ = 0;
        self.current_dnb_pattern = self.base_dnb_pattern;
        self.queued_pattern_id = -1;
        self.pattern_change_queued = false;
        self.evaluate();
    }

    /// Restore the internal state of a running generator with supplied sequencer state
    pub fn restore(&mut self, sequencer_state: SequencerState) {
        self.sequence_step_ = sequencer_state.sequence_step;
        self.euclidean_step = sequencer_state.euclidean_step;
        self.pulse_ = sequencer_state.pulse;
        self.pulse_duration_counter_ = sequencer_state.pulse_duration_counter;
        self.step_ = self.sequence_step_;

        self.first_beat_ = self.sequence_step_ == 0;
        let mut steps_per_beat = K_NUM_STEPS_PER_PATTERN / 4;
        if steps_per_beat == 0 {
            steps_per_beat = 1;
        } // Avoid division by zero for short patterns
        self.beat_ = self.sequence_step_.is_multiple_of(steps_per_beat);

        self.base_dnb_pattern = sequencer_state.base_dnb_pattern;
        self.current_dnb_pattern = sequencer_state.current_dnb_pattern;
        self.queued_pattern_id = -1;
        self.pattern_change_queued = false;
    }

    pub fn retrigger(&mut self) {
        // Re-evaluate the current step without advancing time
        self.evaluate();
    }

    /// Called on each tick of external clock.
    ///
    /// For Drums Mode, this should be fixed to 1/32nd steps
    /// For Euclidean mode, should be in 1/16th steps by default
    /// For DnB mode, it depends on the selected DnB pattern #steps
    ///
    pub fn tick(&mut self, clkn: u32, div: u32) {
        // Derive the sequence step from the absolute clock tick count divided by the clock division,
        // so the generator stays in sync with ticks() rather than tracking its own independent counter.
        let step_count = clkn / div;

        self.sequence_step_ = if self.options_.output_mode == OutputMode::OutputModeDnB {
            (step_count % self.current_dnb_pattern.steps as u32) as u8
        } else {
            (step_count % K_NUM_STEPS_PER_PATTERN as u32) as u8
        };
        self.step_ = self.sequence_step_;

        for part in 0..K_NUM_PARTS {
            self.euclidean_step[part] =
                (step_count % self.current_euclidean_length[part] as u32) as u8;
        }

        self.first_beat_ = self.sequence_step_ == 0;
        let mut steps_per_beat = K_NUM_STEPS_PER_PATTERN / 4;
        if steps_per_beat == 0 {
            steps_per_beat = 1;
        }
        self.beat_ = self.sequence_step_.is_multiple_of(steps_per_beat);

        self.evaluate();
        self.increment_pulse_counter();
    }

    /// Set Euclidean sequence length 1 - 32 steps
    pub fn set_length(&mut self, channel: usize, length: u8) {
        if channel > K_NUM_PARTS {
            return;
        }
        // Clamp length to 1 - 32
        let clamped_length = length.clamp(1, 32);
        self.current_euclidean_length[channel] = clamped_length;
        self.euclidean_offset[channel] %= clamped_length;
    }

    /// Set Euclidean sequence offset for one channel (phase rotation)
    pub fn set_offset(&mut self, channel: usize, offset: u8) {
        if channel > K_NUM_PARTS {
            return;
        }

        let length = self.current_euclidean_length[channel].max(1);
        self.euclidean_offset[channel] = offset % length;
    }

    /// fill_param_value is 0-255 from app
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
                // Default settings for DnB mode
                PatternGeneratorSettings {
                    options: PatternModeSettings::DnB {
                        pattern: 0, // Two-step
                    },
                    density: [255, 255, 255], // Full chaos kick / snare / ghost snare by default
                },
            ],
            current_euclidean_length: [16; K_NUM_PARTS], // Default to 16 steps for all parts in Euclidean mode
            fill: [8; K_NUM_PARTS], // Default fill: 8 steps (50% for a 16-step length)
            euclidean_offset: [0; K_NUM_PARTS],
            random: Random::default(),
            base_dnb_pattern: DnbDrumPattern::default(),
            current_dnb_pattern: DnbDrumPattern::default(),
            queued_pattern_id: -1,
            pattern_change_queued: false,
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
            OutputMode::OutputModeDnB => self.evaluate_dnb(),
        }
    }

    // Evaluates the drum pattern for the current step, applying perturbation based on settings and randomness.
    // Updates the self.state_ with the trigger/accent information for the current step.
    fn evaluate_drums(&mut self) {
        if self.step_ == 0 && self.pulse_ == 0 {
            if self.chaos_globally_enabled_ {
                // Generate perturbation only once at the very start of the 32-step sequence (when pulse_ is also 0)
                let mut randomness =
                    match self.settings_[OutputMode::OutputModeDrums.ordinal() as usize].options {
                        PatternModeSettings::Drums { randomness, .. } => randomness,
                        _ => 0, // Default to 0 if not in Drum mode, though this should never happen
                    };
                randomness >>= 2; // Scale randomness for perturbation amount
                for part in 0..K_NUM_PARTS {
                    self.part_perturbation[part] =
                        u8_u8_mul_shift8(self.random.get_byte(), randomness);
                }
            } else {
                // Ensure no randomisation occurs for drum pattern in next 32-step sequence
                self.part_perturbation = [0; K_NUM_PARTS];
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
            // Invert density 0 - 255 to 255 - 0
            density_thresholds[part] = !*density;
        }

        let mut accent_bits_for_parts: u8 = 0; // Accumulates trigger and accent bits for the current tick
        for (part, threshold) in density_thresholds.iter().enumerate().take(K_NUM_PARTS) {
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
        self.state_ = 0;

        let chaos = match self.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options
        {
            PatternModeSettings::Euclidean { chaos_amount } => chaos_amount,
            _ => 0,
        };

        for part in 0..K_NUM_PARTS {
            let length = self.current_euclidean_length[part].max(2);
            let beats = self.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density
                [part]
                .min(length);

            let offset = self.euclidean_offset[part];
            let pattern_bits = euclidean_pattern(length, beats, offset, 0);
            let pos = self.euclidean_step[part] % length;
            if (pattern_bits >> pos) & 1 == 1 {
                self.state_ |= 1 << part;
            }

            // Chaos: probabilistically flip this beat, with accent injection at high chaos
            if self.chaos_globally_enabled_
                && chaos > 0
                && (self.random.get_word() % 256) < chaos as u16
            {
                if self.random.get_word().is_multiple_of(8) {
                    self.state_ ^= 1 << part;
                }
                if chaos > 192 && self.random.get_word().is_multiple_of(16) {
                    self.state_ |= OutputBits::OutputBitAccent.to_bitmask();
                }
            }
        }
    }

    fn evaluate_dnb(&mut self) {
        if self.first_beat_ && self.pulse_ == 0 {
            // At start of pattern sequence
            if self.pattern_change_queued && self.queued_pattern_id >= 0 {
                // Change pattern sequence if change is queued
                self.generate_dnb_pattern(self.queued_pattern_id as u8);
                self.queued_pattern_id = -1;
                self.pattern_change_queued = false;
            }
        }

        let current_step_in_pattern = self.step_ as usize;
        let mut new_state_for_tick = 0u8; // Accumulates trigger and accent bits for the current tick
        if self.current_dnb_pattern.kick[current_step_in_pattern]
            && self.random.get_byte()
                < self.settings_[OutputMode::OutputModeDnB.ordinal() as usize].density[0]
        {
            new_state_for_tick |= 1 << 0;
        }
        if self.current_dnb_pattern.snare[current_step_in_pattern]
            && self.random.get_byte()
                < self.settings_[OutputMode::OutputModeDnB.ordinal() as usize].density[1]
        {
            new_state_for_tick |= 1 << 1;
        }
        if self.current_dnb_pattern.hihat[current_step_in_pattern] {
            // Hi-hat always triggers (no probability control)
            new_state_for_tick |= 1 << 2;
        }
        if self.current_dnb_pattern.ghost_snare[current_step_in_pattern]
            && self.random.get_byte()
                < self.settings_[OutputMode::OutputModeDnB.ordinal() as usize].density[2]
        {
            new_state_for_tick |= 1 << 3;
        }
        self.state_ = new_state_for_tick; // Update the main trigger state for the current tick
    }

    /// Creates a DnB pattern based on an ID
    fn generate_dnb_pattern(&mut self, pattern_id: u8) {
        let mut p = DnbDrumPattern::default();
        match pattern_id {
            0 => {
                // Two-step
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, true,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, false, false, false,
                    false, true, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            1 => {
                // Delayed Two-Step
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, true,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, false, false, false,
                    false, false, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            2 => {
                // Steppa
                p.has_ghost = true;
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, false, false, true,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.ghost_snare = [
                    false, false, false, false, false, false, false, true, false, true, false,
                    false, false, true, false, true, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            3 => {
                // Extended Steppa
                // @phommed - added and transcribed from Stranjah https://youtu.be/zXGz-M1Fo3g?t=661
                p.has_ghost = true;
                p.kick = [
                    true, false, true, false, false, false, false, false, false, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, true, false, false, true, false, false, true, false, false, true,
                    false, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, true, true, true, true, true, true, true, true, true, true, true, true,
                    true, true, true, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.ghost_snare = [
                    false, false, false, true, true, false, true, true, false, true, true, false,
                    true, true, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            4 => {
                // Stompa
                p.has_ghost = true;
                p.kick = [
                    true, false, false, false, false, false, false, false, true, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, false, false, true,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.ghost_snare = [
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, true, false, true, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            5 => {
                // Dance Hall
                p.kick = [
                    true, false, false, false, false, false, true, false, false, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, true, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            6 => {
                // Dimension UK (double length)
                p.steps = 32;
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false, false, false, false, true, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, true, false, false,
                    false, true, false, false, false, true, false, false, false, true, false,
                    false, false, true, false, false, false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, true, false, true, false,
                ];
            }
            7 => {
                // Halftime
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, false, false, false, false, true, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            8 => {
                // Triplet Two-Step
                p.steps = 24;
                p.kick = [
                    true, false, false, false, false, false, true, false, false, false, false,
                    false, true, false, false, false, false, false, true, false, false, false,
                    false, false, /* end */
                    false, false, false, false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, false, false, true, false, false, false, false,
                    false, false, false, false, false, false, false, true, false, false, false,
                    false, false, /* end */
                    false, false, false, false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, true, false, true, false, true, false, true,
                    false, /* end */
                    false, false, false, false, false, false, false, false,
                ];
            }
            9 => {
                // Amen Break
                p.has_ghost = true;
                p.kick = [
                    true, false, false, false, false, false, false, false, false, false, true,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, true, false, true, false,
                    false, true, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.ghost_snare = [
                    false, false, false, false, false, false, true, false, false, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            10 => {
                // Neurofunk
                p.has_ghost = true;
                p.kick = [
                    true, false, false, false, false, true, false, false, true, false, false,
                    false, false, true, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, true, false, false, false, false, false, false,
                    false, true, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, true, true, false, true, false, true, false, true, true,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.ghost_snare = [
                    false, false, false, true, false, false, false, false, false, false, false,
                    true, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            11 => {
                // Footwork
                // @phommed - added and transcribed from Stranjah https://youtu.be/zXGz-M1Fo3g?t=846
                p.kick = [
                    true, false, false, true, false, false, true, false, true, false, false, true,
                    false, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.snare = [
                    false, false, false, false, false, false, false, false, true, false, false,
                    false, false, false, false, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
                p.hihat = [
                    true, false, true, false, true, false, true, false, true, false, true, false,
                    true, false, true, false, /* end */
                    false, false, false, false, false, false, false, false, false, false, false,
                    false, false, false, false, false,
                ];
            }
            _ => {}
        }
        self.base_dnb_pattern = p;
        self.current_dnb_pattern = p;
    }

    /// Generates a random variation of the current dnb pattern, mutates the self.current_dnb_pattern
    pub fn generate_dnb_variation(&mut self) {
        // Choose a random track (kick, snare, or ghost snare)
        let mut track = self.random.get_byte() % 3; // 0=kick, 1=snare, 2=ghost
        if track >= 2 {
            track = 3; // Map 2 to ghost snare (index 3)
        }

        // Find a position that has a hit in the base pattern
        let (base_track, matched) = match track {
            0 => {
                // Kick
                (self.base_dnb_pattern.kick, true)
            }
            1 => {
                // Snare
                (self.base_dnb_pattern.snare, true)
            }
            3 if self.current_dnb_pattern.has_ghost => {
                // Ghost Snare
                (self.base_dnb_pattern.ghost_snare, true)
            }
            _ => ([false; DNB_MAX_STEPS], false),
        };
        if matched {
            // Find a position that has a hit in the base pattern
            let mut attempts = 0;
            let mut position = 0usize;
            let mut found_hit = false;

            while attempts < self.base_dnb_pattern.steps && !found_hit {
                position = (self.random.get_byte() % self.base_dnb_pattern.steps)
                    .clamp(0, DNB_MAX_STEPS as u8 - 1) as usize;
                if base_track[position] {
                    found_hit = true;
                }
                attempts += 1;
            }

            // Toggle the hit in the current pattern
            if found_hit {
                match track {
                    0 => {
                        self.current_dnb_pattern.kick[position] =
                            !self.current_dnb_pattern.kick[position];
                    }
                    1 => {
                        self.current_dnb_pattern.snare[position] =
                            !self.current_dnb_pattern.snare[position];
                    }
                    3 => {
                        self.current_dnb_pattern.ghost_snare[position] =
                            !self.current_dnb_pattern.ghost_snare[position];
                    }
                    _ => {}
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
        let _ = env_logger::Builder::from_env(Env::default().default_filter_or("warn"))
            .is_test(true)
            .try_init();
    }

    #[test]
    fn test_initialization() {
        let mut generator = PatternGenerator::default();
        generator.reset(); // Ensure reset initializes state correctly
        assert_eq!(0, generator.step_);
        assert_eq!(0, generator.sequence_step_);
        assert_eq!(0, generator.pulse_);
        assert_eq!(true, generator.first_beat_);
        assert_eq!(true, generator.beat_);
        assert_eq!(15, generator.state_);
        assert_eq!(0, generator.sequence_step_);
        assert_eq!(16, generator.base_dnb_pattern.steps);
        assert_eq!(16, generator.current_dnb_pattern.steps);
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
            PatternModeSettings::Drums {
                x: 0,
                y: 0,
                randomness: 0,
            };
        generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].density =
            [31; K_NUM_PARTS];

        generator.evaluate();
        assert_eq!(0, generator.get_step());
        assert_eq!(12, generator.get_trigger_state());

        generator.tick(1, 1);
        assert_eq!(1, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(2, 1);
        assert_eq!(2, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(3, 1);
        assert_eq!(3, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.tick(4, 1);
        assert_eq!(4, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());
    }

    #[test]
    fn test_evaluate_euclidean() {
        init_logger();

        let mut generator: PatternGenerator = PatternGenerator::default();
        generator.options_.output_mode = OutputMode::OutputModeEuclidean;
        generator.options_.gate_mode = true;
        generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].options =
            PatternModeSettings::Euclidean { chaos_amount: 0 };

        // E(3,8): density=3 beats in 8 steps → Bjorklund index 6*33+3=201 → value=73 (0b01001001)
        // fires at steps 0, 3, 6
        for part in 0..K_NUM_PARTS {
            generator.set_length(part, 8);
        }
        generator.settings_[OutputMode::OutputModeEuclidean.ordinal() as usize].density =
            [3; K_NUM_PARTS];

        generator.evaluate();
        assert_eq!(0, generator.get_step());
        assert_eq!(7, generator.get_trigger_state()); // step 0: all 3 parts fire

        generator.tick(1, 1);
        assert_eq!(1, generator.get_step());
        assert_eq!(0, generator.get_trigger_state()); // step 1: no fire

        generator.tick(2, 1);
        assert_eq!(2, generator.get_step());
        assert_eq!(0, generator.get_trigger_state()); // step 2: no fire

        generator.tick(3, 1);
        assert_eq!(3, generator.get_step());
        assert_eq!(7, generator.get_trigger_state()); // step 3: all fire

        generator.tick(4, 1);
        assert_eq!(4, generator.get_step());
        assert_eq!(0, generator.get_trigger_state()); // step 4: no fire

        generator.tick(6, 1);
        assert_eq!(6, generator.get_step());
        assert_eq!(7, generator.get_trigger_state()); // step 6: all fire

        generator.tick(7, 1);
        assert_eq!(7, generator.get_step());
        assert_eq!(0, generator.get_trigger_state()); // step 7: no fire
    }

    #[test]
    fn test_evaluate_dnb() {
        init_logger();
        let mut generator: PatternGenerator = PatternGenerator::default();
        generator.set_seed(0xFFF1);
        generator.options_.output_mode = OutputMode::OutputModeDnB;
        generator.settings_[OutputMode::OutputModeDnB.ordinal() as usize].options =
            PatternModeSettings::DnB { pattern: 0 };
        generator.settings_[OutputMode::OutputModeDrums.ordinal() as usize].density =
            [255; K_NUM_PARTS];
        generator.queue_dnb_pattern_change(0);

        generator.evaluate();
        assert_eq!(0, generator.get_step());
        assert_eq!(5 /* hi-hat and kick */, generator.get_trigger_state());
        generator.tick(1, 1);
        assert_eq!(1, generator.get_step());
        assert_eq!(0, generator.get_trigger_state());

        generator.queue_dnb_pattern_change(2);
        generator.reset();
        generator.evaluate();
        assert_eq!(0, generator.get_step());
        assert_eq!(5 /* hi-hat and kick */, generator.get_trigger_state());
    }

    #[test]
    fn test_tick_with_division() {
        // Verify that step is derived from absolute_tick/div, not from a local counter.
        // With div=3, every 3rd absolute tick corresponds to one sequence step.
        let mut generator: PatternGenerator = PatternGenerator::default();
        generator.set_seed(0xFFF1);
        generator.options_.output_mode = OutputMode::OutputModeEuclidean;

        // tick=3, div=3 → step 1
        generator.tick(3, 3);
        assert_eq!(1, generator.get_step());

        // tick=6, div=3 → step 2
        generator.tick(6, 3);
        assert_eq!(2, generator.get_step());

        // tick=9, div=3 → step 3
        generator.tick(9, 3);
        assert_eq!(3, generator.get_step());

        // tick=0, div=3 → step 0 (absolute position, not relative advance)
        generator.tick(0, 3);
        assert_eq!(0, generator.get_step());

        // tick=15, div=3 → step_count=5
        generator.tick(15, 3);
        assert_eq!(
            5 % K_NUM_STEPS_PER_PATTERN as u32,
            generator.get_step() as u32
        );
    }
}
