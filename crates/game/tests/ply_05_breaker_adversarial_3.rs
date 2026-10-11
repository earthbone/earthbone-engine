// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_05_breaker_seed_boundaries_keep_the_soak_report_reproducible() {
    let seed = u64::MAX;
    let first = crate::ply_02_routes::ply_05_soak(seed);
    let repeated = crate::ply_02_routes::ply_05_soak(seed);
    assert_eq!(first, repeated, "seed {seed}");
    assert!(first.contains(&format!("seed={seed}")), "{first}");
    assert!(first.contains("ticks=5400"), "{first}");
}
