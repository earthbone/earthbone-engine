// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, CompValue, Condition, Schedule, condition};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const ENABLED: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let tree = package::close_header(package::build_template(TEMPLATE, "ConcurrentWaits"), None);
    let tree = package::with_float_input(tree, 5, 1.0);
    let tree = package::with_bool_input(tree, 9, false);
    let tree = package::with_float_input(tree, 6, 2.0);
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
    let mut load_order = LoadOrder::new();
    load_order
        .add(
            "Simultaneous.esp",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    load_order
}

fn game() -> Game {
    let game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_14_wait_deadline_survives_engine_save_load_without_double_time() {
    let mut original = game();
    original.tick(0.25);
    let deadline = original.world().actor_ref(id(ACTOR)).unwrap().ai.wait_until;
    assert_eq!(
        deadline,
        Some(2.25),
        "{:?}",
        original.world().actor_ref(id(ACTOR)).unwrap().ai
    );
    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order());
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai.wait_until,
        deadline
    );

    original.tick(0.5);
    restored.tick(0.5);
    assert_eq!(
        original.world().clock.real_seconds,
        restored.world().clock.real_seconds
    );
    assert_eq!(
        original.world().actor_ref(id(ACTOR)).unwrap().ai,
        restored.world().actor_ref(id(ACTOR)).unwrap().ai
    );
}
