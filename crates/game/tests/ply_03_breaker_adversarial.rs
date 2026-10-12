// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_game::play::{self, Route};

#[test]
fn ply_03_first_route_reaches_its_successor_checkpoint() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 first pair");
        return;
    };
    let data = PathBuf::from(data);
    let folder = PathBuf::from(root).join("Main Quest + Dawnguard(Vampire) + Dragonborn");
    let source = folder.join("01 Right Off The Block.ess");
    let target = folder.join("02 Choose Your Path.ess");
    let mut replay = play::load_world_checkpoint(&data, &source)
        .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
    let mut successor = play::load_world_checkpoint(&data, &target)
        .unwrap_or_else(|error| panic!("{}: {error}", target.display()));
    let route = Route::parse(super::ply_03_main_quest_replay::ROUTES[0]).unwrap();
    let report = play::run_to_checkpoint(&mut replay, &route, &mut successor).unwrap();
    assert!(report.passed(), "{}", report.format());
}
