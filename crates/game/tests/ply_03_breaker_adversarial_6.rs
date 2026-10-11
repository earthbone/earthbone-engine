// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_game::play::{self, Route};

#[test]
fn ply_03_no_vm_error_matches_only_case_insensitive_error_prefix() {
    let mut game = super::ply_02_routes::game(false);
    game.vm.log.push(game.vm.time(), "ERROR: native failure");
    let report = play::run(&mut game, &Route::parse("expect-no-vm-error").unwrap());
    assert!(!report.passed());
    assert!(report
        .failure
        .as_deref()
        .is_some_and(|failure| failure.contains("native failure")));

    let mut game = super::ply_02_routes::game(false);
    game.vm.log.push(game.vm.time(), "warning: error: native failure");
    let report = play::run(&mut game, &Route::parse("expect-no-vm-error").unwrap());
    assert!(report.passed(), "{}", report.format());
}

#[test]
fn ply_03_installed_successor_pairs_match_quest_flags_and_player_inventory() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping installed PLY-03 pairs");
        return;
    };
    let data = PathBuf::from(data);
    let folder = PathBuf::from(root).join("Main Quest + Dawnguard(Vampire) + Dragonborn");
    let saves = [
        "01 Right Off The Block.ess",
        "02 Choose Your Path.ess",
        "03 Followed Ralof.ess",
    ];
    let routes = [
        include_str!("fixtures/ply_03_01.route"),
        include_str!("fixtures/ply_03_02.route"),
    ];
    for index in 0..2 {
        let source = folder.join(saves[index]);
        let target = folder.join(saves[index + 1]);
        let mut replay = play::load_world_checkpoint(&data, &source)
            .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
        let mut successor = play::load_world_checkpoint(&data, &target)
            .unwrap_or_else(|error| panic!("{}: {error}", target.display()));
        let route = Route::parse(routes[index]).unwrap();
        let report = play::run_to_checkpoint(&mut replay, &route, &mut successor).unwrap();
        assert!(
            report.passed(),
            "{} -> {}: {}",
            saves[index],
            saves[index + 1],
            report.format()
        );
    }
}
