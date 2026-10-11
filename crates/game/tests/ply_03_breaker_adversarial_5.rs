// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_game::play::{self, Route};

#[test]
fn ply_03_checkpoint_comparison_keeps_stage_flags_and_inventory_differences() {
    let mut replay = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    let quest = GlobalFormId {
        file: 0,
        object: 0x840,
    };
    let player = replay.player();
    {
        let mut world = replay.world_mut();
        let state = world.quest_state(quest).unwrap();
        state.current = 20;
        state.highest = 20;
        state.done.insert(20);
        state.runtime.other_flags |= 0x20;
        world
            .reference(player)
            .unwrap()
            .inventory
            .add(
                GlobalFormId {
                    file: 0,
                    object: 0x821,
                },
                2,
            )
            .unwrap();
    }
    let report = play::run_to_checkpoint(
        &mut replay,
        &Route::parse("expect-no-vm-error").unwrap(),
        &mut successor,
    )
    .unwrap();
    assert!(!report.passed());
    for field in ["current", "highest", "done", "quest_flags", "player.inventory"] {
        assert!(
            report.differences.iter().any(|difference| difference.contains(field)),
            "missing {field}: {:?}",
            report.differences
        );
    }
}

#[test]
fn ply_03_installed_routes_match_each_ordered_main_quest_successor() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 installed replay");
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
    for index in 0..2 {
        let source = folder.join(saves[index]);
        let target = folder.join(saves[index + 1]);
        let data = PathBuf::from(&data);
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
