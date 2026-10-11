// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-14 simultaneous PACK fixtures use finite Wait children.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const ENABLED: u32 = 0x804;
const MARKER: u32 = 0x807;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(first_wait: f32, second_wait: f32) -> Game {
    game_with_magic(first_wait, second_wait, false)
}

fn game_with_magic(first_wait: f32, second_wait: f32, magic: bool) -> Game {
    game_with_async(first_wait, second_wait, magic, false, false)
}

fn game_with_async(
    first_wait: f32,
    second_wait: f32,
    magic: bool,
    travel: bool,
    travel_completes: bool,
) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let tree = package::close_header(package::build_template(TEMPLATE, "ConcurrentWaits"), None);
    let tree = package::with_float_input(tree, 5, first_wait);
    let tree = package::with_bool_input(tree, 9, false);
    let tree = package::with_float_input(tree, 6, second_wait);
    let tree = package::with_bool_input(tree, 10, true);
    let tree = if travel {
        package::with_location_input(tree, 11, package::Location::NearReference(MARKER), 0)
    } else {
        tree
    };
    let tree = package::finish(tree);
    let mut prcb = (2 + u32::from(magic) + u32::from(travel))
        .to_le_bytes()
        .to_vec();
    prcb.extend_from_slice(&0u32.to_le_bytes());
    let tree = tree.zsub(T(b"ANAM"), "Simultaneous").sub(T(b"PRCB"), prcb);
    let tree = package::with_procedure(tree, "Wait", &[5, 9], false);
    let tree = if magic {
        let tree = package::with_procedure(tree, "UseMagic", &[0], false);
        let tree = package::with_procedure(tree, "Wait", &[6, 10], false);
        package::with_input_name(tree, 0, "Target", true)
    } else {
        package::with_procedure(tree, "Wait", &[6, 10], false)
    };
    let tree = if travel {
        package::with_procedure(tree, "Travel", &[11], travel_completes)
    } else {
        tree
    };
    plugin.push_top(tree);
    let enabled = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: ENABLED,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let mut pack = package::close_header(
        package::build_record(PACK, "Concurrent", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    if magic {
        let target_data = [6u32.to_le_bytes(), 0u32.to_le_bytes(), 0i32.to_le_bytes()].concat();
        pack = pack
            .zsub(T(b"ANAM"), "Target")
            .sub(T(b"PTDA"), target_data)
            .sub(T(b"UNAM"), [0u8]);
    }
    plugin.push_top(package::finish(package::with_condition(pack, &enabled)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&0.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), data),
    );
    if travel {
        let mut position = vec![0; 24];
        position[..4].copy_from_slice(&100.0f32.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"REFR"), MARKER)
                .sub(T(b"NAME"), 0x808u32.to_le_bytes())
                .sub(T(b"DATA"), position),
        );
        plugin.push_top(Record::new(T(b"STAT"), 0x808).zsub(T(b"EDID"), "TravelMarker"));
    }
    plugin.push_top(
        Record::new(T(b"GLOB"), ENABLED)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );
    if magic {
        let mut effect_data = vec![0; 152];
        effect_data[0..4].copy_from_slice(&(1u32 << 1).to_le_bytes());
        effect_data[4..8].copy_from_slice(&7.0f32.to_le_bytes());
        effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        effect_data[64..68].copy_from_slice(&0u32.to_le_bytes());
        effect_data[68..72].copy_from_slice(&24u32.to_le_bytes());
        effect_data[84..88].copy_from_slice(&3u32.to_le_bytes());
        plugin.push_top(Record::new(T(b"MGEF"), 0x805).sub(T(b"DATA"), effect_data));
        let mut spell_data = vec![0; 36];
        spell_data[0..4].copy_from_slice(&7u32.to_le_bytes());
        spell_data[4..8].copy_from_slice(&1u32.to_le_bytes());
        spell_data[8..12].copy_from_slice(&0u32.to_le_bytes());
        spell_data[20..24].copy_from_slice(&3u32.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"SPEL"), 0x806)
                .sub(T(b"SPIT"), spell_data)
                .sub(T(b"EFID"), 0x805u32.to_le_bytes())
                .sub(
                    T(b"EFIT"),
                    [5.0f32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat(),
                ),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add(
            "Simultaneous.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    if magic {
        game.world_mut()
            .set_actor_value(id(ACTOR), "magicka", 20.0)
            .unwrap();
        game.world_mut()
            .actor(id(ACTOR))
            .unwrap()
            .spells
            .insert(id(0x806));
        game.world_mut()
            .equip_hand_spell(id(ACTOR), Hand::Right, id(0x806))
            .unwrap();
    }
    game
}

#[test]
fn ai_14_wait_children_share_one_clock_and_finish_after_the_longest_child() {
    let mut game = game(1.0, 2.0);
    game.tick(0.25);
    assert_eq!(game.world().clock.real_seconds, 0.25);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert!(ai.package.is_some(), "{ai:?}");
    assert_eq!(ai.wait_until, Some(2.25), "{ai:?}");
    assert!(ai.tree.as_ref().unwrap().active);
    game.tick(1.99);
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .active
    );
    game.tick(0.01);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert_eq!(game.world().clock.real_seconds, 2.25);
    assert!(ai.tree.as_ref().unwrap().finished);
    assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
}

#[test]
fn ai_14_package_cancellation_discards_active_simultaneous_children() {
    let mut game = game(1.0, 2.0);
    game.tick(0.25);
    game.world_mut().set_global(id(ENABLED), 0.0);
    assert_eq!(
        game.world_mut()
            .evaluate_package(id(ACTOR), &mut |_, _| panic!("no script binding")),
        None
    );
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert!(ai.tree.is_none());
    assert_eq!(ai.wait_until, None);
    assert!(!ai.completed.contains_key(&id(PACK)));
}

#[test]
fn ai_14_indefinite_wait_child_keeps_the_simultaneous_group_active() {
    let mut game = game(0.0, 2.0);
    game.tick(0.25);
    game.tick(20.0);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert_eq!(ai.wait_until, None);
    assert!(ai.tree.as_ref().unwrap().active);
}

#[test]
fn ai_14_use_magic_child_runs_once_while_wait_child_uses_shared_clock() {
    let mut game = game_with_magic(1.0, 2.0, true);
    game.tick(0.25);
    assert_eq!(game.world().clock.real_seconds, 0.25);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert_eq!(
        game.world_mut().actor_value(id(ACTOR), "magicka"),
        13.0,
        "actor AI: {ai:?}"
    );
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.wait_until,
        Some(2.25)
    );

    let saved = game.save_state().unwrap();
    let mut restored = game_with_magic(1.0, 2.0, true);
    restored.load_state(&saved).unwrap();
    game.tick(1.0);
    restored.tick(1.0);
    assert_eq!(game.world().clock.real_seconds, 1.25);
    assert_eq!(restored.world().clock.real_seconds, 1.25);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 13.0);
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "magicka"), 13.0);
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .active
    );
}

#[test]
fn ai_14_travel_child_completes_with_wait_after_one_shared_clock_advance() {
    let mut game = game_with_async(1.0, 3.0, false, true, false);
    game.tick(0.25);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert_eq!(game.world().clock.real_seconds, 0.25);
    assert_eq!(ai.procedure, soe_game::ai::Procedure::Travel);
    assert_eq!(ai.wait_until, Some(3.25));
    assert!(ai.tree.as_ref().unwrap().simultaneous.is_some());

    let saved = game.save_state().unwrap();
    let mut restored = game_with_async(1.0, 3.0, false, true, false);
    restored.load_state(&saved).unwrap();
    restored.tick(10.0);
    let ai = restored.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert_eq!(restored.world().clock.real_seconds, 10.25);
    assert!(ai.tree.as_ref().unwrap().finished);
    assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
    assert!(restored.world().reference_ref(id(ACTOR)).unwrap().position[0] > 0.0);
}

#[test]
fn ai_14_cancelling_travel_group_discards_route_and_wait_deadline() {
    let mut game = game_with_async(1.0, 3.0, false, true, false);
    game.tick(0.25);
    game.world_mut().set_global(id(ENABLED), 0.0);
    assert_eq!(
        game.world_mut()
            .evaluate_package(id(ACTOR), &mut |_, _| panic!("no script binding")),
        None
    );
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert!(ai.tree.is_none());
    assert_eq!(ai.wait_until, None);
    assert!(!game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(PACK)));
}

#[test]
fn ai_14_successful_travel_child_completes_group_before_longer_waits() {
    let mut game = game_with_async(10.0, 20.0, false, true, true);
    game.tick(1.0);
    let ai = game.world().actor_ref(id(ACTOR)).unwrap().ai.clone();
    assert!(ai.tree.as_ref().unwrap().finished);
    assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
    assert_eq!(game.world().clock.real_seconds, 1.0);
}
