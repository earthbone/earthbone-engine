// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[path = "common/installed.rs"]
mod installed;

use soe_game::Game;

#[test]
fn tst_14_repeated_installed_loads_share_forms_but_not_world_state() {
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let Some(first_order) = installed::load_order(&masters) else {
        eprintln!("SKYRIM_DATA not set, skipping installed cache assertion");
        return;
    };
    let repeated_order = installed::load_order(&masters).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first_order, &repeated_order));
    assert_eq!(
        first_order.files().iter().map(|file| file.name.as_str()).collect::<Vec<_>>(),
        masters
    );

    let first = Game::from_shared_order(first_order);
    let second = Game::from_shared_order(repeated_order);
    let second_hash = second.world().state_hash();
    first.world_mut().perk_points = 11;
    assert_eq!(first.world().perk_points, 11);
    assert_eq!(second.world().perk_points, 0);
    assert_eq!(second.world().state_hash(), second_hash);
}
