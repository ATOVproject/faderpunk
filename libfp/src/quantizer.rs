use crate::{Key, Note, Range};
use libm::roundf;

#[derive(Clone, Copy, PartialEq, Default)]
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

    pub fn as_midi(&self) -> u8 {
        let midi_note = (self.octave as i32 + 1) * 12 + self.note as u8 as i32;
        midi_note.clamp(0, 127) as u8
    }
}

pub struct Quantizer {
    scale_mask: u16,
    // Lookup tables for finding the next/previous in-scale note.
    next_note_dist: [u8; 12],
    prev_note_dist: [u8; 12],
}

impl Quantizer {
    pub fn set_scale(&mut self, key: Key, tonic: Note) {
        let mask = key as u16;
        let shift = tonic as u32;
        // Perform a 12-bit *right* rotation to transpose the scale
        self.scale_mask = ((mask >> shift) | (mask << (12 - shift))) & 0xFFF;

        // Pre-calculate the lookup tables.
        for i in 0..12 {
            // Find distance to the next higher note in the scale
            let mut dist_up = 0;
            while (self.scale_mask & (1 << (11 - ((i + dist_up) % 12)))) == 0 {
                dist_up += 1;
            }
            self.next_note_dist[i as usize] = dist_up;

            // Find distance to the next lower note in the scale
            // Start searching from the note below
            let mut dist_down = 1;
            while (self.scale_mask & (1 << (11 - ((i + 12 - dist_down) % 12)))) == 0 {
                dist_down += 1;
            }
            self.prev_note_dist[i as usize] = dist_down;
        }
    }

    pub fn get_quantized_note(&self, value: u16, range: Range) -> Pitch {
        let input_voltage = match range {
            Range::_0_10V => value as f32 * (10.0 / 4095.0),
            Range::_0_5V => value as f32 * (5.0 / 4095.0),
            Range::_Neg5_5V => (value as f32 * (10.0 / 4095.0)) - 5.0,
        };

        let nearest_semitone = roundf(input_voltage * 12.0) as i32;
        let note_index = nearest_semitone.rem_euclid(12) as usize;

        // First, check if the closest chromatic note is already in the scale
        let final_semitones = if (self.scale_mask & (1 << (11 - note_index))) != 0 {
            nearest_semitone
        } else {
            // If not, find the two surrounding notes and determine which is closer
            let dist_down = self.prev_note_dist[note_index];
            let lower_bound = nearest_semitone - dist_down as i32;

            let dist_up = self.next_note_dist[note_index];
            let upper_bound = nearest_semitone + dist_up as i32;

            let lower_voltage = lower_bound as f32 / 12.0;
            let upper_voltage = upper_bound as f32 / 12.0;

            if (input_voltage - lower_voltage) < (upper_voltage - input_voltage) {
                lower_bound
            } else {
                upper_bound
            }
        };

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
            scale_mask: 0,
            next_note_dist: [0; 12],
            prev_note_dist: [0; 12],
        };
        // Default to C Chromatic
        q.set_scale(Key::Chromatic, Note::C);
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_c_major_unipolar() {
        let mut q = Quantizer::new();
        // C, D, E, F, G, A, B
        q.set_scale(Key::Major, Note::C);

        // 0V -> should be C0
        assert_eq!(
            q.get_quantized_note(0, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // ~1V -> should be C1
        assert_eq!(
            q.get_quantized_note(410, Range::_0_10V),
            Pitch {
                octave: 1,
                note: Note::C
            }
        );

        // Test voltage between C0 (0V) and D0 (0.166V). Midpoint is 0.0833V
        // 0.08V -> ~33 counts. Should snap down to C0
        assert_eq!(
            q.get_quantized_note(33, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // 0.09V -> ~37 counts. Should snap up to D0
        assert_eq!(
            q.get_quantized_note(37, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::D
            }
        );

        // Test voltage between F0 (0.416V) and G0 (0.583V). Midpoint is 0.5V
        // 0.5V -> 205 counts. Should snap up to G0 (rounding up)
        assert_eq!(
            q.get_quantized_note(205, Range::_0_10V),
            Pitch {
                octave: 0,
                note: Note::G
            }
        );
    }

    #[test]
    fn test_quantize_a_minor_bipolar() {
        let mut q = Quantizer::new();
        // A, B, C, D, E, F, G
        q.set_scale(Key::Minor, Note::A);

        // In the key of A minor, C is an in-scale note. 0V is closest to C0.
        // ADC midpoint 2048 should map to 0V.
        assert_eq!(
            q.get_quantized_note(2048, Range::_Neg5_5V),
            Pitch {
                octave: 0,
                note: Note::C
            }
        );

        // -5V -> 0 counts. Should be C-5 (semitone -60).
        // The closest note in A minor to C-5 is... C-5.
        assert_eq!(
            q.get_quantized_note(0, Range::_Neg5_5V),
            Pitch {
                octave: -5,
                note: Note::C
            }
        );

        // ~5V -> 4095 counts. Should be C5 (semitone 60).
        // The closest note in A minor to C5 is C5.
        assert_eq!(
            q.get_quantized_note(4095, Range::_Neg5_5V),
            Pitch {
                octave: 5,
                note: Note::C
            }
        );

        // Test voltage near A4 (4.75V).
        // 4.75V -> ((4.75 + 5.0) / 10.0) * 4095.0 = 3991 counts
        assert_eq!(
            q.get_quantized_note(3991, Range::_Neg5_5V),
            Pitch {
                octave: 4,
                note: Note::A
            }
        );
    }

    #[test]
    fn test_pitch_as_counts() {
        // 4.0V
        let c4 = Pitch {
            octave: 4,
            note: Note::C,
        };
        // 4.75V
        let a4 = Pitch {
            octave: 4,
            note: Note::A,
        };

        // 0-10V range
        // C4 (4.0V) -> (4.0/10.0) * 4095 = 1638
        assert_eq!(c4.as_counts(Range::_0_10V), 1638);
        // A4 (4.75V) -> (4.75/10.0) * 4095 = 1945
        assert_eq!(a4.as_counts(Range::_0_10V), 1945);

        // 0-5V range
        // C4 (4.0V) -> (4.0/5.0) * 4095 = 3276
        assert_eq!(c4.as_counts(Range::_0_5V), 3276);

        // Bipolar -5V to +5V range
        // C4 (4.0V) -> ((4.0 + 5.0)/10.0) * 4095 = 3686
        assert_eq!(c4.as_counts(Range::_Neg5_5V), 3686);
        // C-5 (-5.0V) -> ((-5.0 + 5.0)/10.0) * 4095 = 0
        let c_minus_5 = Pitch {
            octave: -5,
            note: Note::C,
        };
        assert_eq!(c_minus_5.as_counts(Range::_Neg5_5V), 0);
    }

    #[test]
    fn test_pitch_as_midi() {
        // Middle C (C4) should be MIDI note 60
        let c4 = Pitch {
            octave: 4,
            note: Note::C,
        };
        assert_eq!(c4.as_midi(), 60);

        // A4 (440Hz) should be MIDI note 69
        let a4 = Pitch {
            octave: 4,
            note: Note::A,
        };
        assert_eq!(a4.as_midi(), 69);

        // C0 should be MIDI note 12
        let c0 = Pitch {
            octave: 0,
            note: Note::C,
        };
        assert_eq!(c0.as_midi(), 12);

        // C-1 should be MIDI note 0
        let c_minus_1 = Pitch {
            octave: -1,
            note: Note::C,
        };
        assert_eq!(c_minus_1.as_midi(), 0);

        // Test clamping - very high octave should clamp to 127
        let c_high = Pitch {
            octave: 10,
            note: Note::G, // This would be note 139, should clamp to 127
        };
        assert_eq!(c_high.as_midi(), 127);

        // Test clamping - very low octave should clamp to 0
        let c_low = Pitch {
            octave: -10,
            note: Note::C,
        };
        assert_eq!(c_low.as_midi(), 0);
    }
}
