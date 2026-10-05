// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;

use soe_game::play::{self, Route};
use soe_save::Save;

#[test]
#[ignore = "PLY-03 open: the checkpoint replay route has no gameplay step yet, so MQ101 stays at stage 160"]
fn ply_03_replay_must_reach_the_next_main_quest_checkpoint() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping checkpoint replay");
        return;
    };
    let data = PathBuf::from(data);
    let folder = PathBuf::from(root).join("Main Quest + Dawnguard(Vampire) + Dragonborn");
    let source = folder.join("01 Right Off The Block.ess");
    let target = folder.join("02 Choose Your Path.ess");
    let source_save = Save::open(&source).unwrap();
    let target_save = Save::open(&target).unwrap();
    let mut replay = play::load_world_checkpoint(&data, &source).unwrap();
    let report = play::run(&mut replay, &Route::parse("expect-no-vm-error").unwrap());
    assert!(report.passed(), "{}", report.format());

    let mut expected = play::load_world_checkpoint(&data, &target).unwrap();
    let quest_name = "MQ101";
    let actual_stage = replay
        .world()
        .form(quest_name)
        .and_then(|id| replay.world().quest(id).map(|q| q.highest));
    let expected_stage = expected
        .world()
        .form(quest_name)
        .and_then(|id| expected.world().quest(id).map(|q| q.highest));
    assert_ne!(source_save.change_forms.len(), 0);
    assert_ne!(target_save.change_forms.len(), 0);
    let actual_flags = replay.world().form(quest_name).and_then(|id| {
        replay
            .world()
            .quest(id)
            .map(|quest| quest.runtime.other_flags)
    });
    let expected_flags = expected.world().form(quest_name).and_then(|id| {
        expected
            .world()
            .quest(id)
            .map(|quest| quest.runtime.other_flags)
    });
    let actual_player = replay.player();
    let expected_player = expected.player();
    let actual_inventory = replay
        .world()
        .reference_ref(actual_player)
        .unwrap()
        .inventory
        .clone();
    let expected_inventory = expected
        .world()
        .reference_ref(expected_player)
        .unwrap()
        .inventory
        .clone();
    let mut differences = Vec::new();
    if actual_stage != expected_stage {
        differences.push(format!(
            "MQ101.highest: {actual_stage:?} != {expected_stage:?}"
        ));
    }
    if actual_flags != expected_flags {
        differences.push(format!(
            "MQ101.quest_flags: {actual_flags:?} != {expected_flags:?}"
        ));
    }
    if actual_inventory != expected_inventory {
        differences.push("player.inventory differs".to_owned());
    }
    assert!(
        differences.is_empty(),
        "successor differences: {}",
        differences.join(" | ")
    );
}
