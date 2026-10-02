// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! PLY-03 compares two consecutive main quest checkpoint pairs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use soe_esm::GlobalFormId;
use soe_game::play::{self, Route};
use soe_game::Game;
use soe_save::Save;

const CHECKPOINTS: [&str; 3] = [
    "01 Right Off The Block.ess",
    "02 Choose Your Path.ess",
    "03 Followed Ralof.ess",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuestCheckpoint {
    current: u16,
    highest: u16,
    done: Vec<u16>,
    running: bool,
    active: bool,
    completed: bool,
    failed: bool,
    other_flags: u16,
    form_flags: Option<(u32, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Checkpoint {
    quests: BTreeMap<GlobalFormId, QuestCheckpoint>,
    inventory: BTreeMap<GlobalFormId, i32>,
}

fn checkpoint(game: &mut Game, save: &Save) -> Checkpoint {
    let player = game.player();
    let world = game.world();
    let mut quests = BTreeMap::new();
    let mapping = soe_game::vanilla_save::SavedFormMap::new(save, world.order()).unwrap();
    for change in &save.change_forms {
        if change.record_type() != "QUST" {
            continue;
        }
        let Some(id) = mapping.reference(change.form).unwrap() else {
            continue;
        };
        let Some(state) = world.quest(id) else {
            continue;
        };
        quests.insert(
            id,
            QuestCheckpoint {
                current: state.current,
                highest: state.highest,
                done: state.done.iter().copied().collect(),
                running: state.running,
                active: state.active,
                completed: state.completed,
                failed: state.failed,
                other_flags: state.runtime.other_flags,
                form_flags: state.runtime.form_flags,
            },
        );
    }
    let inventory = world
        .reference_ref(player)
        .expect("player reference is present")
        .inventory
        .iter()
        .map(|(item, count)| (*item, count))
        .collect();
    Checkpoint { quests, inventory }
}

fn differences(before: &Checkpoint, expected: &Checkpoint) -> Vec<String> {
    let mut differences = Vec::new();
    for id in before
        .quests
        .keys()
        .chain(expected.quests.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        match (before.quests.get(&id), expected.quests.get(&id)) {
            (Some(left), Some(right)) => {
                for (field, old, new) in [
                    (
                        "current",
                        format!("{}", left.current),
                        format!("{}", right.current),
                    ),
                    (
                        "highest",
                        format!("{}", left.highest),
                        format!("{}", right.highest),
                    ),
                    (
                        "done",
                        format!("{:?}", left.done),
                        format!("{:?}", right.done),
                    ),
                    (
                        "running",
                        format!("{}", left.running),
                        format!("{}", right.running),
                    ),
                    (
                        "active",
                        format!("{}", left.active),
                        format!("{}", right.active),
                    ),
                    (
                        "completed",
                        format!("{}", left.completed),
                        format!("{}", right.completed),
                    ),
                    (
                        "failed",
                        format!("{}", left.failed),
                        format!("{}", right.failed),
                    ),
                    (
                        "quest_flags",
                        format!("{:#06x}", left.other_flags),
                        format!("{:#06x}", right.other_flags),
                    ),
                    (
                        "form_flags",
                        format!("{:?}", left.form_flags),
                        format!("{:?}", right.form_flags),
                    ),
                ] {
                    if old != new {
                        differences.push(format!("quest {id:?}.{field}: {old} -> {new}"));
                    }
                }
            }
            (left, right) => differences.push(format!("quest {id:?}: {left:?} -> {right:?}")),
        }
    }
    for id in before
        .inventory
        .keys()
        .chain(expected.inventory.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let old = before.inventory.get(&id).copied().unwrap_or_default();
        let new = expected.inventory.get(&id).copied().unwrap_or_default();
        if old != new {
            differences.push(format!("player.inventory {id:?}: {old} -> {new}"));
        }
    }
    differences
}

fn save_path(root: &Path, name: &str) -> PathBuf {
    root.join("Main Quest + Dawnguard(Vampire) + Dragonborn")
        .join(name)
}

#[test]
fn ply_03_difference_report_names_stage_quest_flags_and_inventory_fields() {
    let id = GlobalFormId {
        file: 0,
        object: 0x123,
    };
    let before = Checkpoint {
        quests: BTreeMap::from([(
            id,
            QuestCheckpoint {
                current: 10,
                highest: 10,
                done: vec![10],
                running: true,
                active: true,
                completed: false,
                failed: false,
                other_flags: 0x20,
                form_flags: Some((1, 2)),
            },
        )]),
        inventory: BTreeMap::from([(id, 3)]),
    };
    let after = Checkpoint {
        quests: BTreeMap::from([(
            id,
            QuestCheckpoint {
                current: 20,
                highest: 20,
                done: vec![10, 20],
                running: false,
                active: false,
                completed: true,
                failed: true,
                other_flags: 0x40,
                form_flags: Some((3, 4)),
            },
        )]),
        inventory: BTreeMap::from([(id, 7)]),
    };
    let report = differences(&before, &after).join("\n");
    for field in [
        "current",
        "highest",
        "done",
        "running",
        "active",
        "completed",
        "failed",
        "quest_flags",
        "form_flags",
        "player.inventory",
    ] {
        assert!(report.contains(field), "missing {field}: {report}");
    }
}

#[test]
fn ply_03_two_consecutive_main_quest_pairs_list_quest_and_inventory_differences() {
    let (Some(data), Some(corpus)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 checkpoint replay survey");
        return;
    };
    let data = PathBuf::from(data);
    let corpus = PathBuf::from(corpus);
    let route = Route::parse("expect-no-vm-error").unwrap();
    let mut summaries = Vec::new();

    for pair in CHECKPOINTS.windows(2) {
        let source_path = save_path(&corpus, pair[0]);
        let target_path = save_path(&corpus, pair[1]);
        let source_save = Save::open(&source_path).unwrap();
        let mut replay = play::load_world_checkpoint(&data, &source_path)
            .unwrap_or_else(|error| panic!("{}: {error}", source_path.display()));
        let route_report = play::run(&mut replay, &route);
        assert!(route_report.passed(), "{}", route_report.format());
        let played = checkpoint(&mut replay, &source_save);

        let target_save = Save::open(&target_path).unwrap();
        let mut target = play::load_world_checkpoint(&data, &target_path)
            .unwrap_or_else(|error| panic!("{}: {error}", target_path.display()));
        let target_state = checkpoint(&mut target, &target_save);
        let field_differences = differences(&played, &target_state);
        summaries.push(format!(
            "{} -> {}: {} differing field(s)\n{}",
            pair[0],
            pair[1],
            field_differences.len(),
            field_differences.join("\n")
        ));
    }
    assert_eq!(summaries.len(), 2);
    eprintln!(
        "PLY-03 checkpoint field comparison\n{}",
        summaries.join("\n")
    );
}
