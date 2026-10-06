// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Authored PACK branch fixtures, using xEdit PRCB and FNAM layouts.

use crate::common::legacy;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::{world::RngStream, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const C: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn branch(rec: Record, kind: &str, children: u32, repeat: bool) -> Record {
    let mut data = children.to_le_bytes().to_vec();
    data.extend_from_slice(&u32::from(repeat).to_le_bytes());
    rec.zsub(T(b"ANAM"), kind).sub(T(b"PRCB"), data)
}

fn template() -> Record {
    package::finish(package::close_header(
        package::build_template(TEMPLATE, "Sequence"),
        None,
    ))
}

fn travel(rec: Record, input: u8, completes: bool) -> Record {
    package::with_procedure(rec, "Travel", &[input], completes)
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn order(tree: Record, flags: u32) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(tree);
    let p = package::close_header(
        package::build_record(PACK, "Route", flags, Schedule::default()),
        Some(TEMPLATE),
    );
    let p = package::with_location_input(p, 0, Location::NearReference(A), 0);
    let p = package::with_location_input(p, 1, Location::NearReference(B), 0);
    let p = package::with_location_input(p, 2, Location::NearReference(C), 0);
    plugin.push_top(package::finish(p));
    let npc = npc::build_record(
        BASE,
        "Walker",
        "Walker",
        Config::default(),
        Stats::default(),
    );
    plugin.push_top(npc.sub(T(b"PKID"), PACK.to_le_bytes()));
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
    plugin.push_top(placed(*b"REFR", A, 0, 110.0));
    plugin.push_top(placed(*b"REFR", B, 0, 220.0));
    plugin.push_top(placed(*b"REFR", C, 0, 330.0));
    let mut order = LoadOrder::new();
    order
        .add("Sequence.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(tree: Record, flags: u32) -> Game {
    let game = Game::new(order(tree, flags));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn position(game: &mut Game) -> f32 {
    game.world_mut().reference(id(ACTOR)).unwrap().position[0]
}

fn advance(game: &mut Game, ticks: usize) {
    for _ in 0..ticks {
        game.tick(0.1);
    }
}

#[test]
fn a_sequence_runs_each_destination_once_and_only_finishes_after_the_last() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, false), 0, false),
        1,
        false,
    );
    let mut game = game(tree, 0);
    game.tick(2.0);
    assert_eq!(position(&mut game), 110.0, "one child consumes this tick");
    assert!(!game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(PACK)));
    advance(&mut game, 5);
    assert!((position(&mut game) - 165.0).abs() < 0.01);
    advance(&mut game, 10);
    assert_eq!(position(&mut game), 220.0);
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
    assert!(game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(PACK)));
}

#[test]
fn sav_13_engine_save_resumes_saved_sequence_procedure_without_restart() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, false), 0, false),
        1,
        false,
    );
    let mut original = game(tree.clone(), 0);
    original.tick(0.1);
    let saved_cursor = original
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .tree
        .clone();
    assert_eq!(saved_cursor.as_ref().unwrap().branch, 1);
    assert!(saved_cursor.as_ref().unwrap().active);

    let bytes = original.save_state().unwrap();
    let mut restored = Game::new(order(tree, 0));
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai.package,
        Some(id(PACK)),
        "engine save retains the actor's running package"
    );
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai.tree,
        saved_cursor,
        "engine save retains the active procedure branch instead of reevaluating the tree"
    );

    original.tick(0.1);
    restored.tick(0.1);
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai.tree,
        original.world().actor_ref(id(ACTOR)).unwrap().ai.tree,
        "restored driver continues the same branch"
    );
    assert_eq!(
        position(&mut restored),
        position(&mut original),
        "restored actor continues toward the saved procedure target"
    );
}

#[test]
fn nested_sequences_and_root_repeat_keep_their_own_order() {
    let tree = branch(template(), "Sequence", 2, true);
    let tree = travel(branch(tree, "Sequence", 1, false), 0, false);
    let tree = travel(tree, 1, false);
    let mut game = game(tree, 0);
    game.tick(2.0);
    assert_eq!(position(&mut game), 110.0);
    game.tick(2.0);
    assert_eq!(position(&mut game), 220.0);
    game.tick(0.5);
    assert_eq!(
        position(&mut game),
        165.0,
        "root repeated at the first destination"
    );
    assert!(game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .is_empty());
}

#[test]
fn nested_repeat_prevents_the_outer_sibling_from_starting() {
    let tree = branch(template(), "Sequence", 2, false);
    let tree = travel(branch(tree, "Sequence", 1, true), 0, false);
    let tree = travel(tree, 1, false);
    let mut game = game(tree, 0);
    advance(&mut game, 40);
    assert_eq!(position(&mut game), 110.0);
    assert!(game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .is_empty());
}

#[test]
fn success_completes_package_bypasses_later_siblings_and_repeat() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, true), 0, true),
        1,
        false,
    );
    let mut game = game(tree, 0);
    advance(&mut game, 40);
    assert_eq!(position(&mut game), 110.0);
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
}

#[test]
fn failed_branch_conditions_skip_the_whole_subtree() {
    let tree = branch(template(), "Sequence", 2, false);
    let tree = branch(tree, "Sequence", 1, false);
    let dead = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 46,
        param1: 0,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let tree = tree.sub(T(b"CTDA"), dead.to_bytes());
    let tree = travel(travel(tree, 0, false), 1, false);
    let mut game = game(tree, 0);
    game.tick(2.0);
    assert_eq!(position(&mut game), 220.0, "false sequence never visits A");
}

#[test]
fn a_sleep_child_does_not_start_until_travel_completes() {
    let tree = travel(branch(template(), "Sequence", 2, false), 0, false);
    let tree = package::with_procedure(tree, "Sleep", &[1], false);
    let mut game = game(tree, 0);
    game.tick(0.1);
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Travel
    );
    advance(&mut game, 10);
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Sleep
    );
    advance(&mut game, 20);
    assert!(game.world().actor_ref(id(ACTOR)).unwrap().ai.sleeping);
    assert!(game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .is_empty());
}

#[test]
fn save_load_preserves_the_second_child_and_its_path() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, false), 0, false),
        1,
        false,
    );
    let mut game = game(tree.clone(), 0);
    game.tick(2.0);
    game.tick(0.5);
    let bytes = game.save_state().unwrap();
    let before = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    let mut loaded = Game::new(order(tree, 0));
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.world().actor_ref(id(ACTOR)).unwrap().ai, before);
    advance(&mut loaded, 10);
    assert_eq!(position(&mut loaded), 220.0);
}

#[test]
fn empty_repeating_sequences_yield_without_running_later_branches() {
    let tree = branch(template(), "Sequence", 2, false);
    let tree = branch(tree, "Sequence", 0, true);
    let tree = travel(tree, 1, false);
    let mut game = game(tree, 0);
    advance(&mut game, 40);
    assert_eq!(position(&mut game), 0.0);
    assert!(game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .is_empty());
}

#[test]
fn malformed_child_counts_are_rejected_without_recursion() {
    let tree = branch(template(), "Sequence", u32::MAX, false);
    assert!(package::Package::from_record(&tree).is_err());
    let tree = branch(template(), "Procedure", 1, false).zsub(T(b"PNAM"), "Travel");
    assert!(package::Package::from_record(&tree).is_err());
}

#[test]
fn unsupported_group_types_hold_instead_of_running_a_descendant() {
    let tree = travel(branch(template(), "Simultaneous", 1, false), 0, false);
    let mut game = game(tree, 0);
    advance(&mut game, 20);
    assert_eq!(position(&mut game), 0.0);
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported("Simultaneous has an unsupported or failed child procedure".into())
    );
}

#[test]
fn branch_conditions_see_the_package_that_is_entering() {
    let tree = branch(template(), "Sequence", 2, false);
    let tree = travel(tree, 0, false);
    let current = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 161,
        param1: PACK,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let tree = tree.sub(T(b"CTDA"), current.to_bytes());
    let tree = travel(tree, 1, false);
    let mut game = game(tree, 0);
    game.tick(2.0);
    assert_eq!(
        position(&mut game),
        110.0,
        "the first child saw its current package"
    );
}

#[test]
fn once_per_day_waits_for_the_whole_sequence() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, false), 0, false),
        1,
        false,
    );
    let mut game = game(tree, package::flags::ONCE_PER_DAY);
    game.tick(2.0);
    game.tick(0.5);
    assert_eq!(game.world().current_package(id(ACTOR)), Some(id(PACK)));
    advance(&mut game, 30);
    assert_eq!(position(&mut game), 220.0);
    assert_eq!(game.world().current_package(id(ACTOR)), None);
}

#[test]
fn a_world_saved_before_cursors_existed_starts_its_authored_tree() {
    let tree = travel(
        travel(branch(template(), "Sequence", 2, false), 0, false),
        1,
        false,
    );
    let mut game = game(tree, 0);
    let mut bytes = legacy::without_exclusive_branches(&mut game.world_mut(), &[id(ACTOR)]);
    legacy::truncate_to_version(&mut bytes, 20, |candidate| {
        soe_game::World::new(LoadOrder::new())
            .load_state(candidate)
            .is_ok()
    });
    game.world_mut().load_state(&bytes).unwrap();
    game.tick(2.0);
    assert_eq!(position(&mut game), 110.0);
}

#[test]
fn a_corrupt_cursor_is_rejected_before_replacing_the_world() {
    let tree = travel(branch(template(), "Sequence", 1, false), 0, false);
    let mut game = game(tree, 0);
    game.tick(0.1);
    let original = game.world().save_state();
    let mut invalid = original.clone();
    let cursor = game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .tree
        .clone()
        .unwrap();
    let mut marker = 1_u32.to_le_bytes().to_vec();
    marker.extend_from_slice(&id(ACTOR).file.to_le_bytes());
    marker.extend_from_slice(&ACTOR.to_le_bytes());
    marker.push(1);
    marker.extend_from_slice(&cursor.owner.file.to_le_bytes());
    marker.extend_from_slice(&cursor.owner.object.to_le_bytes());
    marker.extend_from_slice(&cursor.branch.to_le_bytes());
    marker.push(u8::from(cursor.active));
    let finished = legacy::unique_offset(&invalid, &marker) + marker.len();
    invalid[finished] = 1;
    assert!(
        game.world_mut()
            .load_state(&invalid)
            .unwrap_err()
            .to_string()
            .contains("finished package tree has an active child"),
        "active and finished conflict"
    );
    assert_eq!(game.world().save_state(), original);
    let mut invalid = original.clone();
    let actor_object = legacy::unique_offset(&invalid, &marker)
        + std::mem::size_of::<u32>()
        + std::mem::size_of::<u16>();
    invalid[actor_object..actor_object + 4].copy_from_slice(&0xffffu32.to_le_bytes());
    assert!(
        game.world_mut()
            .load_state(&invalid)
            .unwrap_err()
            .to_string()
            .contains("package cursor names a missing actor"),
        "cursor actor must exist"
    );
    assert_eq!(game.world().save_state(), original);
}

fn random_pair(repeat: bool) -> Record {
    travel(
        travel(branch(template(), "Random", 2, repeat), 0, false),
        1,
        false,
    )
}

#[test]
fn random_runs_exactly_one_child_and_can_choose_either_child() {
    let mut destinations = std::collections::BTreeSet::new();
    for offset in 0..64 {
        let mut game = game(random_pair(false), 0);
        for _ in 0..offset {
            game.world_mut().random_u64_for(RngStream::BehaviorTree);
        }
        game.tick(0.1);
        let destination = game
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .destination
            .unwrap()[0];
        assert!(destination == 110.0 || destination == 220.0);
        destinations.insert(destination as i32);
        advance(&mut game, 50);
        assert_eq!(
            position(&mut game),
            destination,
            "the unchosen sibling never runs"
        );
        assert!(
            game.world()
                .actor_ref(id(ACTOR))
                .unwrap()
                .ai
                .tree
                .as_ref()
                .unwrap()
                .finished
        );
    }
    assert_eq!(destinations, std::collections::BTreeSet::from([110, 220]));
}

#[test]
fn a_random_child_and_rng_survive_save_load_without_rerolling() {
    let tree = random_pair(true);
    let mut original = game(tree.clone(), 0);
    original.tick(0.1);
    let selected = original
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .tree
        .clone();
    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order(tree, 0));
    restored.load_state(&saved).unwrap();
    for _ in 0..5 {
        original.tick(0.1);
        restored.tick(0.1);
        assert_eq!(
            original.world().actor_ref(id(ACTOR)).unwrap().ai.tree,
            selected
        );
        assert_eq!(
            restored.world().actor_ref(id(ACTOR)).unwrap().ai.tree,
            selected
        );
        assert_eq!(restored.world().save_state(), original.world().save_state());
    }
    assert_eq!(
        restored.world_mut().random_u64_for(RngStream::BehaviorTree),
        original.world_mut().random_u64_for(RngStream::BehaviorTree)
    );
}

#[test]
fn random_filters_immediate_children_by_their_conditions() {
    let tree = travel(branch(template(), "Random", 2, false), 0, false);
    let dead = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 46,
        param1: 0,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let tree = travel(tree.sub(T(b"CTDA"), dead.to_bytes()), 1, false);
    let mut game = game(tree, 0);
    game.tick(3.0);
    assert_eq!(position(&mut game), 220.0);
}

#[test]
fn random_waits_for_its_selected_sequence_before_returning_to_the_parent() {
    let tree = branch(template(), "Sequence", 2, false);
    let tree = branch(tree, "Random", 1, false);
    let tree = branch(tree, "Sequence", 2, false);
    let tree = travel(travel(travel(tree, 0, false), 1, false), 2, false);
    let mut game = game(tree, 0);
    for destination in [110.0, 220.0, 330.0] {
        game.tick(3.0);
        assert_eq!(position(&mut game), destination);
    }
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
}

#[test]
fn a_repeating_random_branch_chooses_again_only_after_child_completion() {
    let tree = random_pair(true);
    let mut game = game(tree, 0);
    let mut destinations = std::collections::BTreeSet::new();
    for _ in 0..32 {
        game.tick(3.0);
        destinations.insert(position(&mut game) as i32);
        let state = game
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .clone()
            .unwrap();
        assert_eq!(state.branch, 0, "completion returns to the random parent");
        assert!(!state.active && !state.finished);
        assert!(game
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .completed
            .is_empty());
    }
    assert_eq!(destinations, std::collections::BTreeSet::from([110, 220]));
}

#[test]
fn random_selection_does_not_evaluate_the_selected_condition_twice() {
    let tree = travel(branch(template(), "Random", 1, false), 0, false);
    let roll = Condition {
        comparison: condition::GREATER_OR_EQUAL,
        flags: 0,
        value: CompValue::Float(0.0),
        function: 77,
        param1: 0,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let tree = tree.sub(T(b"CTDA"), roll.to_bytes());
    let control = game(tree.clone(), 0);
    let mut game = game(tree, 0);
    control.world_mut().random_u64_for(RngStream::Ai);
    control.world_mut().random_u64_for(RngStream::BehaviorTree);
    game.tick(0.1);
    assert_eq!(
        game.world_mut().random_u64_for(RngStream::Ai),
        control.world_mut().random_u64_for(RngStream::Ai),
        "one condition roll, then no additional roll"
    );
    assert_eq!(
        game.world_mut().random_u64_for(RngStream::BehaviorTree),
        control.world_mut().random_u64_for(RngStream::BehaviorTree),
        "one branch selection, then no additional roll"
    );
}

#[test]
fn an_empty_random_branch_completes_or_yields_according_to_its_repeat_flag() {
    for repeat in [false, true] {
        let mut game = game(branch(template(), "Random", 0, repeat), 0);
        advance(&mut game, 10);
        assert_eq!(position(&mut game), 0.0);
        assert_eq!(
            game.world()
                .actor_ref(id(ACTOR))
                .unwrap()
                .ai
                .tree
                .as_ref()
                .unwrap()
                .finished,
            !repeat
        );
    }
}
