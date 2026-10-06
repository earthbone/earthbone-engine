// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for SAV-13 package cursor restoration.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn branch(rec: Record, kind: &str, children: u32) -> Record {
    let mut data = children.to_le_bytes().to_vec();
    data.extend_from_slice(&0_u32.to_le_bytes());
    rec.zsub(T(b"ANAM"), kind).sub(T(b"PRCB"), data)
}

fn template() -> Record {
    package::finish(package::close_header(
        package::build_template(TEMPLATE, "Sequence"),
        None,
    ))
}

fn travel(rec: Record, input: u8) -> Record {
    package::with_procedure(rec, "Travel", &[input], false)
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn order(tree: Record) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(tree);
    let pack = package::close_header(
        package::build_record(PACK, "Route", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let pack = package::with_location_input(pack, 0, Location::NearReference(A), 0);
    let pack = package::with_location_input(pack, 1, Location::NearReference(B), 0);
    plugin.push_top(package::finish(pack));
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
    let mut order = LoadOrder::new();
    order
        .add("Sav13.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn sequence() -> Record {
    let tree = branch(template(), "Sequence", 2);
    travel(travel(tree, 0), 1)
}

fn driven(tree: Record) -> Game {
    let game = Game::new(order(tree));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn position(game: &mut Game) -> f32 {
    game.world_mut().reference(id(ACTOR)).unwrap().position[0]
}

#[test]
fn save_load_mid_second_procedure_preserves_running_package_and_path() {
    let tree = sequence();
    let mut original = driven(tree.clone());
    original.tick(2.0);
    original.tick(0.5);
    let saved_position = position(&mut original);
    assert!(saved_position > 110.0 && saved_position < 220.0);
    let expected_ai = {
        let world = original.world();
        let actor = world.actor_ref(id(ACTOR)).unwrap();
        assert_eq!(actor.ai.tree.as_ref().unwrap().branch, 2);
        assert!(actor.ai.tree.as_ref().unwrap().active);
        actor.ai.clone()
    };

    let mut restored = Game::new(order(tree));
    restored
        .load_state(&original.save_state().unwrap())
        .unwrap();
    assert_eq!(restored.world().current_package(id(ACTOR)), Some(id(PACK)));
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai,
        expected_ai
    );

    for _ in 0..3 {
        let bytes = restored.save_state().unwrap();
        let mut round_trip = Game::new(order(sequence()));
        round_trip.load_state(&bytes).unwrap();
        restored = round_trip;
        original.tick(0.1);
        restored.tick(0.1);
        assert_eq!(
            restored.world().actor_ref(id(ACTOR)).unwrap().ai,
            original.world().actor_ref(id(ACTOR)).unwrap().ai
        );
        assert_eq!(position(&mut restored), position(&mut original));
    }
}

#[test]
fn finished_sequence_stays_finished_after_engine_save_load() {
    let tree = sequence();
    let mut original = driven(tree.clone());
    for _ in 0..50 {
        original.tick(0.1);
    }
    let before = position(&mut original);
    let (saved_tree, completed) = {
        let world = original.world();
        let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
        assert!(ai.tree.as_ref().unwrap().finished);
        (ai.tree.clone(), ai.completed.clone())
    };
    assert!(completed.contains_key(&id(PACK)));

    let mut restored = Game::new(order(tree));
    restored
        .load_state(&original.save_state().unwrap())
        .unwrap();
    for _ in 0..20 {
        restored.tick(0.1);
    }
    {
        let world = restored.world();
        let restored_ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
        assert_eq!(restored_ai.tree, saved_tree);
        assert_eq!(restored_ai.completed, completed);
    }
    assert_eq!(position(&mut restored), before);
}
