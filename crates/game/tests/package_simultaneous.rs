// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-14 simultaneous PACK fixtures use finite Wait children.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const ENABLED: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(first_wait: f32, second_wait: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let tree = package::close_header(package::build_template(TEMPLATE, "ConcurrentWaits"), None);
    let tree = package::with_float_input(tree, 5, first_wait);
    let tree = package::with_bool_input(tree, 9, false);
    let tree = package::with_float_input(tree, 6, second_wait);
    let tree = package::with_bool_input(tree, 10, true);
    let tree = package::finish(tree);
    let mut prcb = 2u32.to_le_bytes().to_vec();
    prcb.extend_from_slice(&0u32.to_le_bytes());
    let tree = tree.zsub(T(b"ANAM"), "Simultaneous").sub(T(b"PRCB"), prcb);
    let tree = package::with_procedure(tree, "Wait", &[5, 9], false);
    let tree = package::with_procedure(tree, "Wait", &[6, 10], false);
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
    let pack = package::close_header(
        package::build_record(PACK, "Concurrent", 0, Schedule::default()),
        Some(TEMPLATE),
    );
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
    plugin.push_top(
        Record::new(T(b"GLOB"), ENABLED)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "Simultaneous.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
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
