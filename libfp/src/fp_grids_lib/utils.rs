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

#[allow(dead_code)]
pub fn u8_mix(a: u8, b: u8, balance: u8) -> u8 {
    ((a as u16 * (255 - balance) as u16 + b as u16 * balance as u16) >> 8) as u8
}

#[allow(dead_code)]
pub fn u8_u8_mul_shift8(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16) >> 8) as u8
}

#[allow(dead_code)]
pub fn u8_u8_mul(a: u8, b: u8) -> u16 {
    a as u16 * b as u16
}

// Ported Random class from avrlib/random.h
#[derive(Debug, Clone, Copy)]
pub struct Random {
    rng_state_: u16,
    s_seeded_: bool, // To ensure srand is called onl;y once by default init
}

#[allow(dead_code)]
impl Random {
    /// Initializes with default settings
    pub fn new() -> Self {
        Self::default()
    }

    pub fn init(&mut self) {
        if !self.s_seeded_ {
            self.rng_state_ = 0xACE1; // Default seed value
            self.s_seeded_ = true;
        }
    }

    pub fn seed(&mut self, seed: u16) {
        self.rng_state_ = seed;
        self.s_seeded_ = true;
    }

    pub fn update(&mut self) {
        // Galois LFSR with feedback polynomial = x^16 + x^14 + x^13 + x^11.
        // Period: 65535.
        self.rng_state_ =
            (self.rng_state_ >> 1) ^ (-((self.rng_state_ & 1) as i16) & 0xb400u16 as i16) as u16;
    }

    pub fn state(&self) -> u16 {
        self.rng_state_
    }

    pub fn state_msb(&self) -> u8 {
        (self.rng_state_ >> 8) as u8
    }

    pub fn get_byte(&mut self) -> u8 {
        self.update();
        self.state_msb()
    }

    pub fn get_control_byte(&mut self) -> u8 {
        ((self.state() >> 8) & 0xFF) as u8
    }

    pub fn get_word(&mut self) -> u16 {
        self.update();
        self.state()
    }
}

impl Default for Random {
    fn default() -> Self {
        Self {
            rng_state_: 0xACE1, // Default seed value
            s_seeded_: true,
        }
    }
}

//
// ************** UNIT TESTS ****************************
//
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_byte() {
        let mut r = Random::new();

        assert_eq!(226, r.get_byte());
        assert_eq!(113, r.get_byte());
        assert_eq!(56, r.get_byte());

        r.init();
        assert_eq!(28, r.get_byte());
        assert_eq!(14, r.get_byte());
        assert_eq!(179, r.get_byte());
        assert_eq!(237, r.get_byte());
        assert_eq!(237, r.get_control_byte());
    }
}
