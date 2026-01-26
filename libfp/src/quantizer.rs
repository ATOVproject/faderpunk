// V/oct quantizer, based on the ideas in
// https://github.com/pichenettes/eurorack/blob/master/braids/quantizer_scales.h

use crate::{Key, MidiNote, Note, Range};
use heapless::Vec;
use libm::roundf;

const CODEBOOK_SIZE: usize = 216;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pitch {
    pub octave: i8,
    pub note: Note,
}

impl Pitch {
    pub fn as_v_oct(&self) -> f32 {
        self.octave as f32 + (self.note as u8 as f32 / 12.0)
    }

    pub fn as_counts(&self, range: Range) -> u16 {
        let voltage = self.as_v_oct();
        let counts = match range {
            Range::_0_10V => (voltage / 10.0) * 4095.0,
            Range::_0_5V => (voltage / 5.0) * 4095.0,
            Range::_Neg5_5V => ((voltage + 5.0) / 10.0) * 4095.0,
        };

        roundf(counts).clamp(0.0, 4095.0) as u16
    }

    pub fn as_midi(&self) -> MidiNote {
        let midi_note = (self.octave as i32 + 1) * 12 + self.note as u8 as i32;
        MidiNote::from(midi_note)
    }
}

pub struct QuantizerState {
    codeword: i16,
    next_boundary: i32,
    previous_boundary: i32,
    version: u64,
}

impl QuantizerState {
    pub fn reset(&mut self, version: u64) {
        // Reset hysteresis when the scale changes
        // Invert boundaries to force a search on the first call
        self.previous_boundary = i32::MAX;
        self.next_boundary = i32::MIN;
        self.codeword = 0;
        self.version = version;
    }
}

impl Default for QuantizerState {
    fn default() -> Self {
        Self {
            codeword: 0,
            previous_boundary: i32::MAX,
            next_boundary: i32::MIN,
            version: 0,
        }
    }
}

pub struct Quantizer {
    codebook: [i16; CODEBOOK_SIZE],
    version: u64,
    key: Key,
    tonic: Note,
}

impl Quantizer {
    pub fn set_scale(&mut self, key: Key, tonic: Note) {
        // Store the key and tonic
        self.key = key;
        self.tonic = tonic;

        let mask = key.as_u16_key();
        let notes: Vec<i16, 12> = (0..12)
            .filter(|i| (mask >> (11 - i)) & 1 != 0) // Read from MSB (C) to LSB (B)
            .map(|i| i as i16)
            .collect();
        if notes.is_empty() {
            // Fallback to chromatic for an empty scale
            self.set_scale(Key::Chromatic, tonic);
            return;
        }

        let tonic_offset = tonic as i16;

        // Build codebook directly with scale notes spanning useful range
        let mut codebook_idx = 0;

        // Cover a wide range of octaves to ensure we can quantize any reasonable input
        for octave in -6..=11 {
            for &note_offset in &notes {
                if codebook_idx >= CODEBOOK_SIZE {
                    break;
                }

                // Calculate semitone: base octave + scale note + tonic transposition
                let semitone = octave * 12 + note_offset + tonic_offset;

                // Convert to fixed-point format and store
                let fixed_point =
                    (semitone as i32 * 128).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                self.codebook[codebook_idx] = fixed_point;
                codebook_idx += 1;
            }
            if codebook_idx >= CODEBOOK_SIZE {
                break;
            }
        }

        // Fill any remaining slots with the last note (highest)
        if codebook_idx > 0 {
            let last_note = self.codebook[codebook_idx - 1];
            for i in codebook_idx..CODEBOOK_SIZE {
                self.codebook[i] = last_note;
            }
        }

        // Sort the codebook for binary search
        self.codebook.sort_unstable();

        self.version = self.version.wrapping_add(1);
    }

    pub fn get_key(&self) -> Key {
        self.key
    }

    pub fn get_tonic(&self) -> Note {
        self.tonic
    }

    pub fn get_quantized_note(
        &self,
        state: &mut QuantizerState,
        value: u16,
        range: Range,
    ) -> Pitch {
        // Version keeps track of the scale changes, if version does not match, reset state
        if state.version != self.version {
            state.reset(self.version);
        }

        let input_voltage = match range {
            Range::_0_10V => value as f32 * (10.0 / 4095.0),
            Range::_0_5V => value as f32 * (5.0 / 4095.0),
            Range::_Neg5_5V => (value as f32 * (10.0 / 4095.0)) - 5.0,
        };

        // Convert voltage to our fixed-point pitch representation (semitones * 128)
        // We assume 1V/Oct, and 0V corresponds to C0 (semitone 0)
        let pitch = roundf(input_voltage * 12.0 * 128.0) as i32;

        if pitch < state.previous_boundary || pitch > state.next_boundary {
            // Input is outside the current note's hysteresis boundary; find a new note
            let upper_bound_index = self.codebook.partition_point(|&x| (x as i32) < pitch);

            let best_index = if upper_bound_index == 0 {
                0
            } else if upper_bound_index >= self.codebook.len() {
                self.codebook.len() - 1
            } else {
                let lower_bound_index = upper_bound_index - 1;
                let dist_lo = (pitch - self.codebook[lower_bound_index] as i32).abs();
                let dist_hi = (pitch - self.codebook[upper_bound_index] as i32).abs();

                if dist_lo <= dist_hi {
                    lower_bound_index
                } else {
                    upper_bound_index
                }
            };

            state.codeword = self.codebook[best_index];

            // Update hysteresis boundaries for the new codeword
            let prev_idx = best_index.saturating_sub(1);
            let next_idx = (best_index + 1).min(self.codebook.len() - 1);
            let prev_codeword = self.codebook[prev_idx] as i32;
            let next_codeword = self.codebook[next_idx] as i32;

            // Weighted average places the boundary closer to the neighbor note
            state.previous_boundary = (9 * prev_codeword + 7 * state.codeword as i32) / 16;
            state.next_boundary = (9 * next_codeword + 7 * state.codeword as i32) / 16;
        }

        let final_semitones = roundf(state.codeword as f32 / 128.0) as i32;
        let octave = final_semitones.div_euclid(12) as i8;
        let note = final_semitones.rem_euclid(12) as u8;

        Pitch {
            octave,
            note: note.into(),
        }
    }
}

impl Default for Quantizer {
    fn default() -> Self {
        let mut q = Self {
            codebook: [0; CODEBOOK_SIZE],
            version: 0,
            // Default to C Chromatic
            key: Key::Chromatic,
            tonic: Note::C,
        };
        q.set_scale(q.get_key(), q.get_tonic());
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_c_major_unipolar() {
        let mut q = Quantizer::default();
        q.set_scale(Key::Ionian, Note::C);
        let mut state = QuantizerState::default();

        // 0V -> should be C0
        assert_eq!(
            q.get_quantized_note(&mut state, 0, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // ~1V -> should be C1
        assert_eq!(
            q.get_quantized_note(&mut state, 410, Range::_0_10V),
            Pitch {
                octave: 1,
                note: Note::C
            }
        );

        // Test voltage between C0 (0V) and D0 (0.166V). Midpoint is ~0.0833V or 34 counts
        // 0.08V -> ~33 counts. Should snap down to C0
        let mut state_c0 = QuantizerState::default();
        assert_eq!(
            q.get_quantized_note(&mut state_c0, 33, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // 0.09V -> ~37 counts. Should snap up to D0
        let mut state_d0 = QuantizerState::default();
        assert_eq!(
            q.get_quantized_note(&mut state_d0, 37, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::D
            }
        );

        // Test voltage between F0 (5 semitones, 0.416V) and G0 (7 semitones, 0.583V).
        // Midpoint is 6 semitones (0.5V), which is F# - not in C Major scale.
        // Should quantize to closest note in scale: either F0 or G0.
        // 0.5V = 205 counts. F0=170 counts, G0=239 counts. 205 is closer to G0
        assert_eq!(
            q.get_quantized_note(&mut state, 205, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::G
            }
        );
    }

    #[test]
    fn test_quantize_a_minor_bipolar() {
        let mut q = Quantizer::default();
        q.set_scale(Key::Aeolian, Note::A);
        let mut state = QuantizerState::default();

        // ADC midpoint 2048 should map to 0V. Closest note in A minor is C0
        assert_eq!(
            q.get_quantized_note(&mut state, 2048, Range::_Neg5_5V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // -5V -> 0 counts. Should be C-5. Closest note is C-5
        assert_eq!(
            q.get_quantized_note(&mut state, 0, Range::_Neg5_5V),
            Pitch {
                octave: -5,
                note: Note::C
            }
        );

        // ~5V -> 4095 counts. Should be C5. Closest note is C5
        assert_eq!(
            q.get_quantized_note(&mut state, 4095, Range::_Neg5_5V),
            Pitch {
                octave: 5,
                note: Note::C
            }
        );
    }

    #[test]
    fn test_full_range() {
        let mut q = Quantizer::default();
        q.set_scale(Key::Chromatic, Note::C);
        let mut state = QuantizerState::default();

        // Test the top of the 0-10V range
        // 10V -> 4095 counts. Should be C10
        assert_eq!(
            q.get_quantized_note(&mut state, 4095, Range::_0_10V),
            Pitch {
                octave: 10,
                note: Note::C
            }
        );

        // Test the bottom of the bipolar -5V to 5V range
        // -5V -> 0 counts. Should be C-5
        assert_eq!(
            q.get_quantized_note(&mut state, 0, Range::_Neg5_5V),
            Pitch {
                octave: -5,
                note: Note::C
            }
        );

        // Test the top of the bipolar -5V to 5V range
        // ~5V -> 4095 counts. Should be C5
        assert_eq!(
            q.get_quantized_note(&mut state, 4095, Range::_Neg5_5V),
            Pitch {
                octave: 5,
                note: Note::C
            }
        );
    }

    #[test]
    fn test_hysteresis() {
        let mut q = Quantizer::default();
        // C, D, E, F, G, A, B
        q.set_scale(Key::Ionian, Note::C);
        let mut state = QuantizerState::default();

        // The midpoint between C0 (0V) and D0 (~0.167V) is ~0.083V or 34 counts
        // A stateless quantizer would snap 33->C0 and 37->D0

        // Quantize a value just ABOVE the midpoint. It should snap to D0
        assert_eq!(
            q.get_quantized_note(&mut state, 37, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::D
            }
        );

        // Quantize a value just BELOW the midpoint (33 counts)
        // A stateless quantizer would snap back to C0
        // With hysteresis, it should STAY on D0 because it hasn't crossed the new, lower boundary
        assert_eq!(
            q.get_quantized_note(&mut state, 33, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::D
            }
        );

        // Only when we provide a value far away from the boundary does it snap back
        // 10 counts is ~0.024V, clearly closer to C0
        assert_eq!(
            q.get_quantized_note(&mut state, 10, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );
    }

    // The Pitch helper function tests are unaffected by the quantizer change
    #[test]
    fn test_pitch_as_counts() {
        let c4 = Pitch {
            octave: 4,
            note: Note::C,
        };
        let a4 = Pitch {
            octave: 4,
            note: Note::A,
        };
        assert_eq!(c4.as_counts(Range::_0_10V), 1638);
        assert_eq!(a4.as_counts(Range::_0_10V), 1945);
        assert_eq!(c4.as_counts(Range::_0_5V), 3276);
        assert_eq!(c4.as_counts(Range::_Neg5_5V), 3686);
    }

    #[test]
    fn test_pitch_as_midi() {
        let c4 = Pitch {
            octave: 4,
            note: Note::C,
        };
        assert_eq!(c4.as_midi(), MidiNote(60));
        let a4 = Pitch {
            octave: 4,
            note: Note::A,
        };
        assert_eq!(a4.as_midi(), MidiNote(69));
        let c_minus_1 = Pitch {
            octave: -1,
            note: Note::C,
        };
        assert_eq!(c_minus_1.as_midi(), MidiNote(0));
    }

    #[test]
    fn test_get_key_and_tonic() {
        let mut q = Quantizer::default();

        // Check default values
        assert_eq!(q.get_key(), Key::Chromatic);
        assert_eq!(q.get_tonic(), Note::C);

        // Set a new scale and verify getters return correct values
        q.set_scale(Key::Ionian, Note::D);
        assert_eq!(q.get_key(), Key::Ionian);
        assert_eq!(q.get_tonic(), Note::D);

        // Try another scale
        q.set_scale(Key::Aeolian, Note::A);
        assert_eq!(q.get_key(), Key::Aeolian);
        assert_eq!(q.get_tonic(), Note::A);

        // Try all 12 tonics
        for tonic_val in 0..12 {
            let tonic: Note = unsafe { core::mem::transmute(tonic_val as u8) };
            q.set_scale(Key::Dorian, tonic);
            assert_eq!(q.get_tonic(), tonic);
            assert_eq!(q.get_key(), Key::Dorian);
        }
    }

    #[test]
    fn test_get_key_tonic_after_empty_scale_fallback() {
        let mut q = Quantizer::default();

        // Create an empty scale (all bits zero)
        let empty_key: Key = unsafe { core::mem::transmute(0u8) };

        // Set scale with empty key, should fallback to Chromatic
        q.set_scale(empty_key, Note::E);

        // After fallback, key should be Chromatic and tonic should be preserved
        assert_eq!(q.get_key(), Key::Chromatic);
        assert_eq!(q.get_tonic(), Note::E);
    }

    #[test]
    fn test_key_tonic_preserved_during_quantization() {
        let mut q = Quantizer::default();
        q.set_scale(Key::Mixolydian, Note::G);
        let mut state = QuantizerState::default();

        // Perform some quantization operations
        q.get_quantized_note(&mut state, 410, Range::_0_10V);
        q.get_quantized_note(&mut state, 820, Range::_0_10V);
        q.get_quantized_note(&mut state, 1230, Range::_0_10V);

        // Key and tonic should remain unchanged
        assert_eq!(q.get_key(), Key::Mixolydian);
        assert_eq!(q.get_tonic(), Note::G);
    }
}
