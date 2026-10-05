// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::GlobalFormId;

#[test]
fn ply_06_missing_package_does_not_take_player_control() {
    let Some(mut game) = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA unset, skipping missing package handoff check");
        return;
    };
    let player = game.player();
    let missing = GlobalFormId {
        file: u16::MAX,
        object: u32::MAX,
    };
    let before = game.world().count_package_overrides(player);

    assert!(!game.world_mut().add_package_override(player, missing, 100, 1));
    assert_eq!(game.world().count_package_overrides(player), before);
    assert!(!game.world().player_ai_driven);
}
