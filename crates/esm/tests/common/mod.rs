// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

//! Seeds and reproduction lines for the property tests.
//!
//! `TEST_PLAN.md` section 5 sets one rule for every tier: a failure prints its
//! seed, the offset or tick it failed at, and a single command that replays
//! it. Nothing here is clever, it exists so that rule is followed the same way
//! in every property test rather than reinvented per file.
//!
//! Two kinds of randomness need two answers. A `proptest!` block owns its own
//! sampling and writes the shrunk case to `proptest-regressions/`, so replay
//! is a matter of keeping that file and running the test again, and [`repro`]
//! says so. A test that draws its own numbers (a tick to save at, a byte to
//! corrupt) picks its seed through [`seed`], which honours `SOE_SEED` so the
//! printed command genuinely replays the run.

use std::fmt::Write as _;

/// The environment variable that pins [`seed`].
pub const SEED_VAR: &str = "SOE_SEED";

/// The seed for a test that draws its own numbers.
///
/// Reads [`SEED_VAR`] when set, otherwise takes one from the clock. Either way
/// it prints the value, so a failing run in someone's terminal carries the
/// number needed to repeat it even when the assertion message is lost.
pub fn seed(test: &str) -> u64 {
    let chosen = match std::env::var(SEED_VAR) {
        Ok(given) => given
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{SEED_VAR} is {given:?}, which is not a u64")),
        Err(_) => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED),
    };
    eprintln!("{test}: {SEED_VAR}={chosen}");
    chosen
}

/// A small deterministic generator, so a seed replays exactly.
///
/// This is `SplitMix64`. The property tests need a handful of bounded numbers
/// per case and nothing statistical, so a dependency for it would not pay for
/// itself.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..limit`. Panics on an empty range, which is a bug in the
    /// caller rather than a case worth generating.
    pub fn below(&mut self, limit: usize) -> usize {
        assert!(limit > 0, "below(0) has no answer");
        (self.next_u64() % limit as u64) as usize
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        (self.next_u64() % 100) < u64::from(percent)
    }
}

/// The command that replays a failure, for a test that uses [`seed`].
pub fn repro_seeded(krate: &str, test_file: &str, test_name: &str, seed: u64) -> String {
    format!(
        "{SEED_VAR}={seed} cargo test -p {krate} --test {test_file} {test_name} -- --exact --nocapture"
    )
}

/// The command that replays a `proptest!` failure, which lives in the
/// regression file rather than in a seed.
pub fn repro(krate: &str, test_file: &str, test_name: &str) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "cargo test -p {krate} --test {test_file} {test_name} -- --exact --nocapture"
    );
    let _ = write!(
        out,
        " (the shrunk case is in crates/{}/tests/proptest-regressions/{test_file}.txt, keep it)",
        krate.trim_start_matches("soe-")
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{Rng, SEED_VAR};

    #[test]
    fn the_generator_replays_from_a_seed() {
        let a: Vec<u64> = (0..8).map(|_| Rng::new(42).next_u64()).collect();
        let mut one = Rng::new(42);
        let mut two = Rng::new(42);
        assert_eq!(one.next_u64(), two.next_u64());
        assert_eq!(a[0], a[7], "the same seed gives the same first draw");
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(7);
        for _ in 0..256 {
            assert!(rng.below(5) < 5);
        }
    }

    #[test]
    fn the_seed_variable_is_the_one_the_repro_line_prints() {
        assert_eq!(SEED_VAR, "SOE_SEED");
    }
}
