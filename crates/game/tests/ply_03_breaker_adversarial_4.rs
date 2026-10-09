// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_game::play::{self, Route};

#[test]
#[ignore = "needs PLY-03: the Helgen trigger and the MQ101 scene progression, see docs/game.md"]
fn ply_03_each_route_reaches_its_successor_checkpoint() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 replay");
        return;
    };
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
    let mut failures = Vec::new();
    for index in 0..2 {
        let source = folder.join(saves[index]);
        let successor = folder.join(saves[index + 1]);
        let mut replay = play::load_world_checkpoint(&PathBuf::from(&data), &source)
            .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
        let mut expected = play::load_world_checkpoint(&PathBuf::from(&data), &successor)
            .unwrap_or_else(|error| panic!("{}: {error}", successor.display()));
        let route = Route::parse(routes[index]).unwrap();
        let report = play::run_to_checkpoint(&mut replay, &route, &mut expected).unwrap();
        if !report.passed() {
            failures.push(format!(
                "{} -> {}: {}",
                saves[index],
                saves[index + 1],
                report.format()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
