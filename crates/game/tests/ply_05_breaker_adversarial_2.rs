// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_05_zero_seed_still_walks_and_survives_the_full_soak() {
    let report = crate::ply_02_routes::ply_05_soak(0);
    assert!(report.contains("ticks=5400"), "{report}");
    assert!(report.contains("completed="), "{report}");
}
