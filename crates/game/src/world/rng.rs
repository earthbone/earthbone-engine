// SPDX-License-Identifier: GPL-3.0-or-later

//! Seed-derived xorshift streams for independent runtime systems.

use super::World;

/// A subsystem whose random draws must not shift another subsystem's stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum RngStream {
    /// AI package evaluation and movement.
    Ai = 0,
    /// Dialogue response selection.
    Dialogue = 1,
    /// Mod-data distribution.
    ModData = 2,
    /// Leveled item selection.
    LeveledItems = 3,
    /// Actor template selection.
    ActorTemplates = 4,
    /// Leveled spell selection.
    ActorSpells = 5,
    /// PACK behavior tree selection.
    BehaviorTree = 6,
}

pub(super) const STREAM_COUNT: usize = 7;

pub(super) fn derive_streams(seed: u64) -> [u64; STREAM_COUNT] {
    let mut state = seed;
    std::array::from_fn(|_| {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let value = value ^ (value >> 31);
        if value == 0 {
            1
        } else {
            value
        }
    })
}

impl World {
    /// Draw the next value from a subsystem's independent stream.
    pub fn random_u64_for(&mut self, stream: RngStream) -> u64 {
        let state = &mut self.rng_streams[stream as usize];
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    /// Draw an inclusive integer from a subsystem's independent stream.
    pub fn random_int_for(&mut self, stream: RngStream, min: i32, max: i32) -> i32 {
        let (lo, hi) = (min.min(max), min.max(max));
        let span = (hi as i64 - lo as i64 + 1) as u64;
        lo + (self.random_u64_for(stream) % span) as i32
    }

    /// Draw a float from a subsystem's independent stream.
    pub fn random_float_for(&mut self, stream: RngStream, min: f32, max: f32) -> f32 {
        let unit = (self.random_u64_for(stream) >> 11) as f64 / (1u64 << 53) as f64;
        min + (max - min) * unit as f32
    }

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
