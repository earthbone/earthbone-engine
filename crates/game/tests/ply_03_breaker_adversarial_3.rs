// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_game::play::{self, Checkpoint, Route};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
#[ignore = "needs PLY-03: the Helgen trigger and the MQ101 scene progression, see docs/game.md"]
fn ply_03_replay_reaches_both_consecutive_corpus_checkpoints() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 corpus replay");
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
        let target = folder.join(saves[index + 1]);
        let mut replay = play::load_world_checkpoint(&PathBuf::from(&data), &source)
            .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
        let mut successor = play::load_world_checkpoint(&PathBuf::from(&data), &target)
            .unwrap_or_else(|error| panic!("{}: {error}", target.display()));
        let route = Route::parse(routes[index]).unwrap();
        let report = play::run_to_checkpoint(&mut replay, &route, &mut successor).unwrap();
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

#[test]
fn ply_03_union_scope_compares_successor_only_quest_to_source_defaults() {
    let mut replay = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    let quest = id(0x840);
    successor.world_mut().quest_state(quest).unwrap().highest = 10;
    let report = play::run_to_checkpoint(
        &mut replay,
        &Route::parse("expect-no-vm-error").unwrap(),
        &mut successor,
    )
    .unwrap();
    assert!(!report.passed());
    assert!(report
        .differences
        .iter()
        .any(|field| field.contains("highest")));
}

#[test]
fn ply_03_absent_and_zero_inventory_quantities_compare_equal() {
    let mut game = super::ply_02_routes::game(false);
    let player = game.player();
    let inventory = game
        .world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .clone();
    let absent = Checkpoint::capture(&mut game, &[]).unwrap();
    game.world_mut().reference(player).unwrap().inventory = inventory;
    let zeroed = Checkpoint::capture(&mut game, &[]).unwrap();
    assert_eq!(absent.differences(&zeroed), Vec::<String>::new());
}

#[test]
fn ply_03_failed_route_still_reports_successor_inventory_differences() {
    let mut replay = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    let player = successor.player();
    successor
        .world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .add(id(0x821), 1)
        .unwrap();
    let report = play::run_to_checkpoint(
        &mut replay,
        &Route::parse("activate Skyrim.esm:999999").unwrap(),
        &mut successor,
    )
    .unwrap();
    assert!(!report.passed());
    assert!(report.route.failure.is_some());
    assert!(report
        .differences
        .iter()
        .any(|field| field.contains("inventory")));
}
