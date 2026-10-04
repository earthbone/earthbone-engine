// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_06_replacing_an_override_keeps_one_entry_and_reselects_fallback() {
    let Some(mut game) = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA unset, skipping installed package overrides");
        return;
    };
    let player = game.player();
    let primary = game.form("WhiterunAdrianneEstablishingScene").unwrap();
    let fallback = game.form("C06EorlundReforgeWuuthrad").unwrap();

    assert!(game.world_mut().add_package_override(player, primary, 60, 1));
    assert!(game.world_mut().add_package_override(player, fallback, 40, 1));
    assert!(game.world_mut().add_package_override(player, primary, 40, 1));
    assert_eq!(game.world().count_package_overrides(player), 2);
    assert_eq!(
        soe_game::ai::select(&mut game.world_mut(), player).unwrap().package,
        primary,
        "latest equal-priority override must run"
    );
    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    assert_eq!(game.world().count_package_overrides(player), 2);
    assert_eq!(
        soe_game::ai::select(&mut game.world_mut(), player).unwrap().package,
        primary,
        "saved override ordering must survive engine save/load"
    );

    assert!(game.world_mut().remove_package_override(player, primary));
    assert_eq!(
        soe_game::ai::select(&mut game.world_mut(), player).unwrap().package,
        fallback,
        "removing winner must reveal next eligible package"
    );
}
