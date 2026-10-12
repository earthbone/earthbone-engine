// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let tree = package::close_header(package::build_template(TEMPLATE, "EqualWaits"), None);
    let tree = package::with_float_input(tree, 5, 1.0);
    let tree = package::with_bool_input(tree, 9, false);
    let tree = package::with_float_input(tree, 6, 1.0);
    let tree = package::with_bool_input(tree, 10, false);
    let tree = package::finish(tree);
    let mut prcb = 2u32.to_le_bytes().to_vec();
    prcb.extend_from_slice(&0u32.to_le_bytes());
    let tree = tree.zsub(T(b"ANAM"), "Simultaneous").sub(T(b"PRCB"), prcb);
    let tree = package::with_procedure(tree, "Wait", &[5, 9], false);
    let tree = package::with_procedure(tree, "Wait", &[6, 10], false);
    plugin.push_top(tree);
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACK, "EqualWaits", 0, Schedule::default()),
        Some(TEMPLATE),
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
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order
        .add("EqualWaits.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_14_equal_wait_children_finish_once_on_the_shared_deadline() {
    let mut game = game();
    game.tick(0.25);
    let ai = &game.world().actor_ref(id(ACTOR)).unwrap().ai;
    assert_eq!(ai.wait_until, Some(1.25));
    assert!(ai.tree.as_ref().unwrap().active);

    game.tick(0.5);
    game.tick(0.5);
    let ai = &game.world().actor_ref(id(ACTOR)).unwrap().ai;
    assert_eq!(game.world().clock.real_seconds, 1.25);
    assert!(ai.tree.as_ref().unwrap().finished);
    assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
}

#[test]
fn ai_14_saved_wait_group_keeps_one_deadline_after_restore() {
    let mut original = game();
    original.tick(0.25);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();

    original.tick(0.75);
    restored.tick(0.75);
    for current in [&original, &restored] {
        let ai = &current.world().actor_ref(id(ACTOR)).unwrap().ai;
        assert_eq!(current.world().clock.real_seconds, 1.0);
        assert_eq!(ai.wait_until, Some(1.25));
        assert!(ai.tree.as_ref().unwrap().active);
        assert!(!ai.completed.contains_key(&id(PACK)));
    }

    original.tick(0.25);
    restored.tick(0.25);
    for current in [&original, &restored] {
        let ai = &current.world().actor_ref(id(ACTOR)).unwrap().ai;
        assert_eq!(current.world().clock.real_seconds, 1.25);
        assert!(ai.tree.as_ref().unwrap().finished);
        assert_eq!(ai.completed.get(&id(PACK)), Some(&0));
    }
}
