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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut simultaneous = 2u32.to_le_bytes().to_vec();
    simultaneous.extend_from_slice(&0u32.to_le_bytes());
    let tree = package::close_header(package::build_template(TEMPLATE, "WaitAndWait"), None);
    let tree = package::with_float_input(tree, 5, 0.25);
    let tree = package::with_bool_input(tree, 9, false);
    let tree = package::with_float_input(tree, 6, 2.0);
    let tree = package::with_bool_input(tree, 10, true);
    let tree = package::finish(tree)
        .zsub(T(b"ANAM"), "Simultaneous")
        .sub(T(b"PRCB"), simultaneous);
    let tree = package::with_procedure(tree, "Wait", &[5, 9], true);
    let tree = package::with_procedure(tree, "Wait", &[6, 10], false);
    plugin.push_top(tree);
    let pack = package::close_header(
        package::build_record(PACK, "ConcurrentRoute", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::finish(pack));
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
        .add(
            "WaitAndWait.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_14_completing_wait_cancels_the_concurrent_wait_child() {
    let mut game = fixture();
    game.tick(0.25);
    game.tick(0.25);
    let world = game.world();
    assert_eq!(world.clock.real_seconds, 0.5);
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert_eq!(ai.completed.get(&id(PACK)), Some(&0), "{ai:?}");
    assert!(ai.tree.as_ref().unwrap().finished);
}
