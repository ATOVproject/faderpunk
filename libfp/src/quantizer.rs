use heapless::Vec;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Key {
    Chromatic = 0b111111111111,
    Major = 0b101011010101,
    Minor = 0b101101011010,
    PentatonicMajor = 0b101010010100,
    PentatonicMinor = 0b100101010010,
    Purvi = 0b110010111001,
    Todi = 0b110100111001,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Note {
    C = 0,
    CSharp = 1,
    D = 2,
    DSharp = 3,
    E = 4,
    F = 5,
    FSharp = 6,
    G = 7,
    GSharp = 8,
    A = 9,
    ASharp = 10,
    B = 11,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pitch {
    pub octave: usize,
    pub note: Note,
}

impl Default for Pitch {
    fn default() -> Self {
        Self {
            octave: 0,
            note: Note::C,
        }
    }
}

impl Pitch {
    /// Convert pitch to a voltage (1V/oct standard)
    pub fn as_v_oct(&self) -> f32 {
        self.octave as f32 + (self.note as u8 as f32 / 12.0)
    }
}

#[derive(Default)]
pub struct Quantizer<const N: usize> {
    current_scale: Vec<Pitch, N>,
}

impl<const N: usize> Quantizer<N> {
    pub fn set_scale(&mut self, key: Key, root: Note, tonic: Note) {
        let octave_pattern = key as u16;

        let root_value = root as i8;
        let tonic_value = tonic as i8;

        // Calculate shift based on tonic and root
        let mut shift = (tonic_value - root_value) % 12;
        if shift < 0 {
            shift += 12;
        }

        // Shift the pattern for the tonic
        let shifted_pattern = if shift > 0 {
            ((octave_pattern >> shift) | (octave_pattern << (12 - shift))) & 0xFFF
        } else {
            octave_pattern
        };

        self.current_scale.clear();

        // Build the scale
        for i in 0..N {
            let bit_pos = i % 12;
            if (shifted_pattern & (1 << (11 - bit_pos))) != 0 {
                // Calculate the actual note and octave
                let semitones_from_root = root_value as i32 + i as i32;
                let octave = (semitones_from_root / 12) as usize;
                let note_value = semitones_from_root % 12;

                // Handle negative values and convert to appropriate Note enum
                let note = match (note_value + 12) % 12 {
                    0 => Note::C,
                    1 => Note::CSharp,
                    2 => Note::D,
                    3 => Note::DSharp,
                    4 => Note::E,
                    5 => Note::F,
                    6 => Note::FSharp,
                    7 => Note::G,
                    8 => Note::GSharp,
                    9 => Note::A,
                    10 => Note::ASharp,
                    11 => Note::B,
                    _ => unreachable!(),
                };

                let _ = self.current_scale.push(Pitch { octave, note });
            }
        }
    }

    pub fn get_quantized_note(&self, value: u16) -> Pitch {
        if self.current_scale.is_empty() {
            return Pitch::default();
        }

        // Normalize value from 0-4095 range to scale index
        // Using integer math only (no floating point)
        let scale_len = self.current_scale.len();

        // Handle the special case where value is exactly 4095
        if value == 4095 {
            return self.current_scale[scale_len - 1];
        }

        // Calculate index using integer division and multiplication
        // This avoids floating point rounding issues
        let index = ((value as usize * scale_len) / 4096).clamp(0, scale_len - 1);

        self.current_scale[index]
    }

    // For backward compatibility, now takes u16 instead of f32
    pub fn get_quantized_voltage(&self, value: u16) -> f32 {
        let Pitch { octave, note } = self.get_quantized_note(value);

        // Convert back to voltage using the 1V/octave standard
        octave as f32 + (note as u8 as f32 / 12.0)
    }
}

// class QuantizerClass {
//  private:
//   float currentScale_[61] = {0};
//   uint8_t currentScaleLength_ = 0;
//
//  public:
//   QuantizerClass();
//   void SetScale(Note tonic, Key key, Note root, uint8_t octave, uint8_t range);
//   float GetQuantizedVoltage(float value);
// };
//
// float QuantizerClass::GetQuantizedVoltage(float value) {
//   uint8_t index = round(value * (float)currentScaleLength_);
//   return currentScale_[index];
// }
//
// // Octave: 0 (C0) - Range - 1
// // Range: 0 (1 Octave + 1) - 4 (5 Octaves + 1)
// void QuantizerClass::SetScale(Note tonic, Key key, Note root, uint8_t octave, uint8_t range) {
//   uint64_t scale = keys[key];
//   uint8_t pos = 0;
//   currentScaleLength_ = 0;
//
//   // Shift scale for tonic and root appropriately
//   int8_t shift = tonic - root;
//   if (shift > 0) {
//     scale = ror(scale, shift, 60);
//   }
//   if (shift < 0) {
//     scale = rol(scale, abs(shift), 60);
//   }
//   // Assemble scale
//   while (pos < (range + 1) * 12) {
//     if (scale & (1ULL << (59 - pos))) {
//       currentScale_[currentScaleLength_++] =
//           (float)octave + (float)(root + pos) * QUANT_STEP_SEMITONE;
//     }
//     pos++;
//   }
//   // We add the root value to the end if it's a 1 (x Octaves + 1)
//   if (scale & (1ULL << 59)) {
//     currentScale_[currentScaleLength_] =
//         (float)octave + (float)(root + pos) * QUANT_STEP_SEMITONE;
//   }
// }
