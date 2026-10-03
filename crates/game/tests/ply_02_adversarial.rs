// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for authored route assertions and navigation endpoints.

use crate::ply_02_routes::{game, game_with, id, run};
use soe_esm::{Record, Tag};

#[test]
fn ply_02_adversarial_navigation_projects_at_128_units_and_rejects_beyond_it() {
    let mut at_limit = game(false);
    let report = run(
        &mut at_limit,
        "navigate-marker Skyrim.esm:800 0 0 128 120 0",
    );
    assert!(report.passed(), "{}", report.format());
    assert_eq!(report.ticks, 0);
    assert_eq!(
        at_limit.world().reference_ref(id(0x14)).unwrap().position,
        [0.0, 0.0, 0.0]
    );

    let mut beyond_limit = game(false);
    let report = run(
        &mut beyond_limit,
        "navigate-marker Skyrim.esm:800 0 0 128.01 120 10",
    );
    assert!(!report.passed(), "{}", report.format());
    assert_eq!(report.ticks, 0);
    assert!(report.failure.unwrap().contains("128 units"));
}

#[test]
fn ply_02_adversarial_position_tolerance_is_euclidean_and_cell_specific() {
    let mut within = game(false);
    let report = run(&mut within, "expect-position Skyrim.esm:800 0.3 0.4 0 0.5");
    assert!(report.passed(), "{}", report.format());

    let mut outside = game(false);
    let report = run(
        &mut outside,
        "expect-position Skyrim.esm:800 0.3 0.4 0.01 0.5",
    );
    assert!(!report.passed(), "{}", report.format());
    assert!(report.failure.unwrap().contains("distance"));
}

#[test]
fn ply_02_adversarial_disabled_reference_and_cyclic_item_list_fail_cleanly() {
    let mut disabled = game(false);
    disabled.world_mut().reference(id(0x810)).unwrap().enabled = false;
    let report = run(&mut disabled, "navigate Skyrim.esm:810 120 10");
    assert!(!report.passed(), "{}", report.format());
    assert_eq!(report.ticks, 0);
    assert!(report.failure.unwrap().contains("unavailable"));
    assert_eq!(
        disabled.world().reference_ref(id(0x14)).unwrap().position,
        [0.0; 3]
    );

    let mut cyclic = game_with(false, |plugin| {
        plugin.push_top(
            Record::new(Tag(*b"FLST"), 0x1901).sub(Tag(*b"LNAM"), 0x1902u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(Tag(*b"FLST"), 0x1902).sub(Tag(*b"LNAM"), 0x1901u32.to_le_bytes()),
        );
    });
    let report = run(&mut cyclic, "expect-item player Skyrim.esm:1901 0");
    assert!(!report.passed(), "{}", report.format());
    assert!(report.failure.unwrap().contains("cyclic form list"));
    assert!(report.state.contains("player"));
}
