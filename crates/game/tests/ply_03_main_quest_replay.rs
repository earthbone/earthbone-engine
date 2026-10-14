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

#[test]
fn ply_03_saved_tower_scene_presents_headtracking_and_reaches_the_entry_trigger() {
    let (Some(data), Some(corpus)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 tower entry");
        return;
    };
    let mut game = play::load_world_checkpoint(
        &PathBuf::from(data),
        &save_path(&PathBuf::from(corpus), CHECKPOINTS[0]),
    )
    .unwrap();
    let scene = GlobalFormId {
        file: 0,
        object: 0xd0594,
    };
    let quest = GlobalFormId {
        file: 0,
        object: 0xd0593,
    };
    assert_eq!(game.world().scene_phase(scene), Some(2));
    let (speaker, target) = {
        let mut world = game.world_mut();
        (
            world.alias_ref(quest, 13).unwrap(),
            world.alias_ref(quest, 12).unwrap(),
        )
    };
    assert_eq!(
        game.world_mut().actor_look_at(speaker),
        Some((target, false))
    );
    let route = Route::parse(
        "play-scene Skyrim.esm:d0594 3 600\n\
         navigate Skyrim.esm:10757b 600 3600\n\
         wait-stage MQ101DragonAttack 20 600\n\
         play-scene Skyrim.esm:d0594 4 600\n\
         expect-stage MQ101DragonAttack 20\n\
         navigate-marker Skyrim.esm:982b 15636.233 -81284.97 8205.542 600 3600\n\
         navigate-marker Skyrim.esm:982a 17970.816 -80446.69 8234.532 600 3600\n\
         wait-stage MQ101DragonAttack 160 60\n\
         expect-no-vm-error",
    )
    .unwrap();
    let report = play::run(&mut game, &route);
    assert!(report.passed(), "{}", report.format());
    assert_eq!(game.world().scene_phase(scene), Some(4));
    assert!(game.world().quest(quest).unwrap().done.contains(&20));
    assert!(game.world().quest(quest).unwrap().done.contains(&160));
    assert!(game.world_mut().actor_look_at(speaker).is_none());
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
fn ply_03_no_vm_error_assertion_matches_error_prefix_case_insensitively() {
    let mut game = super::ply_02_routes::game(false);
    game.vm
        .log
        .push(game.vm.time(), "Error: expected native error");
    let report = play::run(&mut game, &Route::parse("expect-no-vm-error").unwrap());
    assert!(!report.passed());
    assert!(report
        .failure
        .as_deref()
        .is_some_and(|failure| failure.contains("expected native error")));
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
fn ply_03_installed_route_stages_belong_to_the_named_quest() {
    use soe_game::play::Step;

    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping PLY-03 route stage definitions");
        return;
    };
    let plugin = soe_esm::read_plugin(PathBuf::from(data).join("Skyrim.esm"))
        .unwrap()
        .unwrap();
    let mut order = soe_esm::LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let game = soe_game::Game::new(order);
    for text in ROUTES {
        for (line, step) in Route::parse(text).unwrap().steps {
            let (Step::WaitStage { quest, stage, .. } | Step::ExpectStage { quest, stage }) = step
            else {
                continue;
            };
            let mut world = game.world_mut();
            let id = world.form(&quest).unwrap();
            assert!(
                world.quest_state(id).unwrap().def.stage(stage).is_some(),
                "route line {line}: {quest} has no stage {stage}"
            );
        }
    }
}

#[test]
fn ply_03_authored_routes_contain_gameplay_and_wait_for_successor_stages() {
    use soe_game::play::Step;
    for ((text, successor), dragon_stage) in ROUTES.into_iter().zip([180, 250]).zip([192, 195]) {
        let route = Route::parse(text).unwrap();
        assert!(route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::Navigate { .. } | Step::Activate(_) | Step::Talk { .. }
        )));
        assert!(route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::WaitStage { quest, stage, .. }
                if (*stage == successor && quest == "MQ101")
        )));
        assert!(route.steps.iter().any(|(_, step)| matches!(
            step,
            Step::WaitStage { quest, stage, .. }
                if (*stage == dragon_stage && quest == "MQ101DragonAttack")
        )));
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

#[test]
fn ply_03_saved_keep_scene_phase_start_runs_stage_174_fragment() {
    let (Some(data), Some(corpus)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 keep scene fragment");
        return;
    };
    let source = save_path(&PathBuf::from(corpus), CHECKPOINTS[1]);
    let mut game = play::load_world_checkpoint(&PathBuf::from(data), &source).unwrap();
    let scene = GlobalFormId {
        file: 0,
        object: 0xdedbb,
    };
    let quest = GlobalFormId {
        file: 0,
        object: 0xd0593,
    };
    assert_eq!(game.world().scene_phase(scene), Some(5));
    game.tick(0.0);
    assert_eq!(game.world().scene_phase(scene), Some(5));
    assert!(!game.world().quest(quest).unwrap().done.contains(&174));
    let record = game.world().record(scene).unwrap().clone();
    let definition = soe_esm::records::gameplay::Scene::from_record(&record).unwrap();
    {
        let world = game.shared_world();
        let mut allocate = |name: &str, class: &str| game.vm.create_object_of(name, class);
        for phase in [5, 6] {
            for (slot, action) in definition.actions.iter().enumerate() {
                if action.end_phase == Some(phase) {
                    assert!(world.borrow_mut().complete_scene_action(scene, slot as u32));
                }
            }
            assert!(world.borrow_mut().advance_scene_phase(scene, &mut allocate));
        }
    }
    assert_eq!(game.world().scene_phase(scene), Some(7));
    game.tick(0.0);
    assert!(
        game.world().quest(quest).unwrap().done.contains(&174),
        "{:?}",
        game.vm.log.entries
    );
}

#[test]
fn ply_03_second_route_reaches_its_successor_checkpoint() {
    let (Some(data), Some(corpus)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping PLY-03 second pair");
        return;
    };
    let data = PathBuf::from(data);
    let corpus = PathBuf::from(corpus);
    let mut replay =
        play::load_world_checkpoint(&data, &save_path(&corpus, CHECKPOINTS[1])).unwrap();
    let mut successor =
        play::load_world_checkpoint(&data, &save_path(&corpus, CHECKPOINTS[2])).unwrap();
    let route = Route::parse(ROUTES[1]).unwrap();
    let report = play::run_to_checkpoint(&mut replay, &route, &mut successor).unwrap();
    assert!(report.passed(), "{}", report.format());
}
