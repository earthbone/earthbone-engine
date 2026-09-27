// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, CompValue, Condition, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const C: u32 = 0x806;
const FIRST: u32 = 0x807;
const SECOND: u32 = 0x808;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn branch(rec: Record, kind: &str, children: u32) -> Record {
    rec.zsub(T(b"ANAM"), kind)
        .sub(T(b"PRCB"), [children as u8, 0, 0, 0, 0, 0, 0, 0])
}

fn travel(rec: Record, input: u8) -> Record {
    package::with_procedure(rec, "Travel", &[input], false)
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), form).sub(T(b"NAME"), base.to_le_bytes()).sub(T(b"DATA"), data)
}

fn condition(global: u32) -> Condition {
    Condition {
        comparison: package::condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: global,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    }
}

fn game(tree: Record) -> Game {
    let package_record = package::close_header(
        package::build_record(PACK, "StackedPriority", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let mut package_record = package::with_location_input(package_record, 0, Location::NearReference(A), 0);
    package_record = package::with_location_input(package_record, 1, Location::NearReference(B), 0);
    package_record = package::with_location_input(package_record, 2, Location::NearReference(C), 0);
    let package_record = package::finish(package_record);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(tree);
    plugin.push_top(package_record);
    plugin.push_top(npc::build_record(BASE, "Walker", "Walker", Config::default(), Stats::default()).sub(T(b"PKID"), PACK.to_le_bytes()));
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
    plugin.push_top(placed(*b"REFR", A, 0, 110.0));
    plugin.push_top(placed(*b"REFR", B, 0, 220.0));
    plugin.push_top(placed(*b"REFR", C, 0, 330.0));
    for global in [FIRST, SECOND] {
        plugin.push_top(Record::new(T(b"GLOB"), global).sub(T(b"FNAM"), b"f").sub(T(b"FLTV"), 0.0f32.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("StackedPriority.esm", Plugin::parse(&plugin.to_bytes()).unwrap()).unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_15_multiple_higher_priority_children_run_once_then_resume_saved_route() {
    let stacked = branch(package::finish(package::close_header(package::build_template(TEMPLATE, "StackedPriority"), None)), "Stacked", 3);
    let first = travel(stacked, 0).sub(T(b"CTDA"), condition(FIRST).to_bytes());
    let second = travel(first, 1).sub(T(b"CTDA"), condition(SECOND).to_bytes());
    let tree = travel(second, 2);
    let mut game = game(tree.clone());
    game.tick(0.5);
    game.world_mut().set_global(id(FIRST), 1.0);
    game.tick(0.1);
    game.world_mut().set_global(id(SECOND), 1.0);

    let saved = game.save_state().unwrap();
    let mut loaded = Game::new({
        let mut order = Plugin::new(FileHeader::default());
        order.push_top(tree.clone());
        let mut p = package::close_header(package::build_record(PACK, "StackedPriority", 0, Schedule::default()), Some(TEMPLATE));
        p = package::with_location_input(p, 0, Location::NearReference(A), 0);
        p = package::with_location_input(p, 1, Location::NearReference(B), 0);
        p = package::with_location_input(p, 2, Location::NearReference(C), 0);
        order.push_top(package::finish(p));
        order.push_top(npc::build_record(BASE, "Walker", "Walker", Config::default(), Stats::default()).sub(T(b"PKID"), PACK.to_le_bytes()));
        order.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
        order.push_top(placed(*b"REFR", A, 0, 110.0));
        order.push_top(placed(*b"REFR", B, 0, 220.0));
        order.push_top(placed(*b"REFR", C, 0, 330.0));
        for global in [FIRST, SECOND] { order.push_top(Record::new(T(b"GLOB"), global).sub(T(b"FNAM"), b"f").sub(T(b"FLTV"), 1.0f32.to_le_bytes())); }
        let mut load_order = LoadOrder::new();
        load_order.add("StackedPriority.esm", Plugin::parse(&order.to_bytes()).unwrap()).unwrap();
        load_order
    });
    loaded.load_state(&saved).unwrap();
    for _ in 0..60 { game.tick(0.5); loaded.tick(0.5); }
    assert!((game.world_mut().reference(id(ACTOR)).unwrap().position[0] - 330.0).abs() < 0.01);
    assert_eq!(loaded.world().save_state(), game.world().save_state());
}
