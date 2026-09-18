// SPDX-License-Identifier: GPL-3.0-or-later

//! The world's random number stream, one xorshift state seeded at
//! construction and carried by the save, what `Utility.RandomInt` and
//! `RandomFloat` draw from.

use super::World;

impl World {
    pub fn random_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    pub fn random_int(&mut self, min: i32, max: i32) -> i32 {
        let (lo, hi) = (min.min(max), min.max(max));
        let span = (hi as i64 - lo as i64 + 1) as u64;
        lo + (self.random_u64() % span) as i32
    }

    pub fn random_float(&mut self, min: f32, max: f32) -> f32 {
        let unit = (self.random_u64() >> 11) as f64 / (1u64 << 53) as f64;
        min + (max - min) * unit as f32
    }
}
