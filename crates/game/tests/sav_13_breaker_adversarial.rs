// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the live package query and engine save boundary.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn load_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(package::finish(package::close_header(
        package::build_template(TEMPLATE, "Sequence"),
        None,
    )));
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACK, "Route", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    let npc = npc::build_record(
        BASE,
        "Walker",
        "Walker",
        Config::default(),
        Stats::default(),
    );
    plugin.push_top(npc.sub(Tag(*b"PKID"), PACK.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "Sav13Breaker.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    order
}

#[test]
fn current_package_reports_the_live_actor_package_and_no_missing_actor() {
    let mut game = Game::new(load_order());
    assert_eq!(game.world().current_package(id(ACTOR)), None);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    assert_eq!(game.world().current_package(id(ACTOR)), Some(id(PACK)));
    assert_eq!(game.world().current_package(id(0x8ff)), None);
}

#[test]
fn live_package_identity_survives_repeated_engine_save_loads() {
    let mut game = Game::new(load_order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    for _ in 0..3 {
        let bytes = game.save_state().unwrap();
        let mut restored = Game::new(load_order());
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.world().current_package(id(ACTOR)), Some(id(PACK)));
        game = restored;
    }
}
