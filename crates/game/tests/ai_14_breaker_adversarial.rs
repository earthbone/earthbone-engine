// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

fn order(first_wait: f32, second_wait: f32) -> LoadOrder {
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
    plugin.push_top(package::finish(package::with_condition(
        package::close_header(
            package::build_record(PACK, "Concurrent", 0, Schedule::default()),
            Some(TEMPLATE),
        ),
        &enabled,
    )));
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
            "Simultaneous.esp",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    order
}

fn game(first_wait: f32, second_wait: f32) -> Game {
    let game = Game::new(order(first_wait, second_wait));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_14_split_ticks_and_save_load_complete_at_the_longest_deadline() {
    let mut original = game(1.0, 2.0);
    original.tick(0.25);
    original.tick(0.0);
    original.tick(0.0);
    assert_eq!(original.world().clock.real_seconds, 0.25);
    assert_eq!(
        original.world().actor_ref(id(ACTOR)).unwrap().ai.wait_until,
        Some(2.25)
    );

    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order(1.0, 2.0));
    restored.load_state(&saved).unwrap();
    original.tick(0.75);
    restored.tick(0.75);
    assert!(
        original
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .active
    );
    assert!(
        restored
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .active
    );
    original.tick(1.25);
    restored.tick(1.25);
    for current in [&original, &restored] {
        let world = current.world();
        let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
        assert_eq!(current.world().clock.real_seconds, 2.25);
        assert!(ai.tree.as_ref().unwrap().finished);
        assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
    }
}

#[test]
fn ai_14_cancelled_group_does_not_complete_after_its_old_deadline() {
    let mut game = game(1.0, 2.0);
    game.tick(0.25);
    game.world_mut().set_global(id(ENABLED), 0.0);
    assert_eq!(
        game.world_mut()
            .evaluate_package(id(ACTOR), &mut |_, _| panic!("no script binding")),
        None
    );
    game.tick(10.0);
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(ai.tree.is_none());
    assert_eq!(ai.wait_until, None);
    assert!(!ai.completed.contains_key(&id(PACK)));
}

#[test]
fn ai_14_completed_group_keeps_its_completion_through_save_load() {
    let mut original = game(0.5, 1.0);
    original.tick(0.25);
    original.tick(1.0);
    assert_eq!(
        original
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .completed
            .get(&id(PACK)),
        Some(&0)
    );
    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order(0.5, 1.0));
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .completed
            .get(&id(PACK)),
        Some(&0)
    );
}
