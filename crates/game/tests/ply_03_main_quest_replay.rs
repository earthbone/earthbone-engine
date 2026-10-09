// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! PLY-03 compares two consecutive main quest checkpoint pairs.

use soe_game::play::{Checkpoint, QuestCheckpoint};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use soe_esm::GlobalFormId;
use soe_game::play::{self, Route};

pub(super) const ROUTES: [&str; 2] = [
    include_str!("fixtures/ply_03_01.route"),
    include_str!("fixtures/ply_03_02.route"),
];

const CHECKPOINTS: [&str; 3] = [
    "01 Right Off The Block.ess",
    "02 Choose Your Path.ess",
    "03 Followed Ralof.ess",
];

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
    let report = before.differences(&after).join("\n");
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
#[ignore = "needs PLY-03: the Helgen trigger and the MQ101 scene progression, see docs/game.md"]
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
    let mut summaries = Vec::new();

    let mut failures = Vec::new();
    for (index, pair) in CHECKPOINTS.windows(2).enumerate() {
        let route = Route::parse(ROUTES[index]).unwrap();
        let source_path = save_path(&corpus, pair[0]);
        let target_path = save_path(&corpus, pair[1]);
        let mut replay = play::load_world_checkpoint(&data, &source_path)
            .unwrap_or_else(|error| panic!("{}: {error}", source_path.display()));
        let mut target = play::load_world_checkpoint(&data, &target_path)
            .unwrap_or_else(|error| panic!("{}: {error}", target_path.display()));
        let report = play::run_to_checkpoint(&mut replay, &route, &mut target).unwrap();
        if !report.passed() {
            failures.push(format!("{} -> {}: {}", pair[0], pair[1], report.format()));
        }
        let field_differences = report.differences;
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
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ply_03_checkpoint_capture_reads_record_defaults_without_loading_quests() {
    let mut plugin = super::ply_01_player_input::plugin();
    plugin.push_top(soe_game::quest::build_record(0x844, "UnloadedQuest", 0));
    let mut order = soe_esm::LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = soe_game::Game::new(order);
    let player = game.player();
    game.world_mut().reference(player).unwrap();
    let quest = GlobalFormId {
        file: 0,
        object: 0x844,
    };
    assert!(game.world().quest(quest).is_none());
    let before = game.world().save_state();
    let first = Checkpoint::capture(&mut game, &[quest, quest]).unwrap();
    let second = Checkpoint::capture(&mut game, &[quest]).unwrap();
    assert_eq!(first, second);
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn ply_03_checkpoint_capture_rejects_an_invalid_quest_scope() {
    let mut game = super::ply_02_routes::game(false);
    let error = Checkpoint::capture(
        &mut game,
        &[GlobalFormId {
            file: 0,
            object: 0x800,
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid quest"));
}

#[test]
fn ply_03_gameplay_route_reaches_an_independent_successor_inventory() {
    let route = Route::parse("activate Skyrim.esm:811\nactivate Skyrim.esm:820\nexpect-item player Skyrim.esm:821 1\nexpect-no-vm-error").unwrap();
    let mut game = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    let player = successor.player();
    successor
        .world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .add(
            GlobalFormId {
                file: 0,
                object: 0x821,
            },
            1,
        )
        .unwrap();
    let report = play::run_to_checkpoint(&mut game, &route, &mut successor).unwrap();
    assert!(report.passed(), "{}", report.format());
    assert!(report.route.ticks > 0);
}

#[test]
fn ply_03_gameplay_script_reaches_successor_stages_and_flags() {
    use soe_papyrus::asm::{b, i, ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
    let mut game = super::ply_02_routes::game(false);
    let script = ScriptBuilder::new("CheckpointActivation", "ObjectReference")
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("activator", "ObjectReference")
                .local("quest", "Quest")
                .local("result", "Bool")
                .local("unused", "None")
                .ins(
                    CallStatic,
                    vec![
                        s("Game"),
                        s("GetFormFromFile"),
                        ident("quest"),
                        i(0x840),
                        s("Skyrim.esm"),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![s("SetStage"), ident("quest"), ident("result"), i(10)],
                )
                .ins(
                    CallMethod,
                    vec![s("SetActive"), ident("quest"), ident("unused"), b(false)],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let target = GlobalFormId {
        file: 0,
        object: 0x820,
    };
    let object = game.object_of_form(target);
    game.vm.attach(object, "CheckpointActivation").unwrap();
    let mut successor = super::ply_02_routes::game(false);
    let player = successor.player();
    successor
        .world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .add(
            GlobalFormId {
                file: 0,
                object: 0x821,
            },
            1,
        )
        .unwrap();
    {
        let mut world = successor.world_mut();
        let state = world
            .quest_state(GlobalFormId {
                file: 0,
                object: 0x840,
            })
            .unwrap();
        state.current = 10;
        state.highest = 10;
        state.done.insert(10);
        state.active = false;
    }
    let route = Route::parse("activate Skyrim.esm:811\nactivate Skyrim.esm:820\nwait-stage PlyQuest 10 60\nexpect-no-vm-error").unwrap();
    let report = play::run_to_checkpoint(&mut game, &route, &mut successor).unwrap();
    assert!(report.passed(), "{}", report.format());
    assert!(report.route.ticks > 0);
}

#[test]
fn ply_03_checkpoint_mismatch_fails_even_when_route_assertions_pass() {
    let mut game = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    let quest = GlobalFormId {
        file: 0,
        object: 0x840,
    };
    successor
        .world_mut()
        .quest_state(quest)
        .unwrap()
        .done
        .insert(10);
    successor.world_mut().quest_state(quest).unwrap().highest = 10;
    let route = Route::parse("expect-no-vm-error").unwrap();
    let report = play::run_to_checkpoint(&mut game, &route, &mut successor).unwrap();
    assert!(report.route.passed());
    assert!(!report.passed());
    assert!(report
        .differences
        .iter()
        .any(|field| field.contains(".highest:")));
    assert!(report
        .differences
        .iter()
        .any(|field| field.contains(".done:")));
}

#[test]
fn ply_03_failed_route_keeps_checkpoint_differences_and_failure_evidence() {
    let mut game = super::ply_02_routes::game(false);
    let mut successor = super::ply_02_routes::game(false);
    successor
        .world_mut()
        .quest_state(GlobalFormId {
            file: 0,
            object: 0x840,
        })
        .unwrap()
        .highest = 20;
    let report = play::run_to_checkpoint(
        &mut game,
        &Route::parse("expect-stage PlyQuest 10").unwrap(),
        &mut successor,
    )
    .unwrap();
    assert!(!report.passed());
    assert!(report.format().contains("FAIL step 1"));
    assert!(report.format().contains("checkpoint:"));
    assert!(report
        .differences
        .iter()
        .any(|field| field.contains(".highest:")));
}

#[test]
fn ply_03_inventory_comparison_treats_missing_quantities_as_zero_and_lists_items() {
    let id = GlobalFormId {
        file: 0,
        object: 0x821,
    };
    let empty = Checkpoint {
        quests: BTreeMap::new(),
        inventory: BTreeMap::new(),
    };
    let zero = Checkpoint {
        quests: BTreeMap::new(),
        inventory: BTreeMap::from([(id, 0)]),
    };
    assert!(empty.differences(&zero).is_empty());
    let changed = Checkpoint {
        quests: BTreeMap::new(),
        inventory: BTreeMap::from([(id, 2)]),
    };
    assert_eq!(
        empty.differences(&changed),
        vec![format!("player.inventory {id:?}: 0 -> 2")]
    );
    assert_eq!(
        changed.differences(&empty),
        vec![format!("player.inventory {id:?}: 2 -> 0")]
    );
}

#[test]
fn ply_03_authored_routes_contain_gameplay_and_wait_for_successor_stages() {
    use soe_game::play::Step;
    for (text, successor) in ROUTES.into_iter().zip([180, 250]) {
        let route = Route::parse(text).unwrap();
        assert!(route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::Navigate { .. } | Step::Activate(_) | Step::Talk { .. }
        )));
        assert!(route
            .steps
            .iter()
            .any(|(_, step)| matches!(step, Step::WaitStage { stage, .. } if *stage == successor)));
    }
}

#[test]
fn ply_03_difference_order_is_stable_and_missing_quests_name_the_presence_field() {
    let mut game = super::ply_02_routes::game(false);
    let id = GlobalFormId {
        file: 0,
        object: 0x840,
    };
    let before = Checkpoint::capture(&mut game, &[id]).unwrap();
    let after = Checkpoint {
        quests: BTreeMap::new(),
        inventory: BTreeMap::new(),
    };
    assert_eq!(
        before.differences(&after),
        vec![format!("quest {id:?}.present: true -> false")]
    );
    assert_eq!(
        after.differences(&before),
        vec![format!("quest {id:?}.present: false -> true")]
    );
}
