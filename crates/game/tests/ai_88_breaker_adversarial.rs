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
const A: u32 = 0x804;
const B: u32 = 0x805;
const TOPIC: u32 = 0x806;
const FURNITURE: u32 = 0x808;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn ptdo() -> Vec<u8> {
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend_from_slice(&TOPIC.to_le_bytes());
    data
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut template = package::close_header(
        package::build_template(TEMPLATE, "PatrolFurnitureSave"),
        None,
    );
    let mut target = 0i32.to_le_bytes().to_vec();
    target.extend_from_slice(&A.to_le_bytes());
    target.extend_from_slice(&0i32.to_le_bytes());
    template = template
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), target)
        .sub(T(b"UNAM"), [7]);
    template = package::with_float_input(template, 11, 0.0);
    template = package::with_bool_input(template, 3, false);
    template = package::with_bool_input(template, 9, false);
    template = package::with_bool_input(template, 14, false);
    template = package::finish(package::with_bool_input(template, 15, false));
    template = package::with_procedure(template, "Patrol", &[7, 11, 3, 9, 14, 15], false);
    plugin.push_top(template);
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACK, "Patrol", 0, Schedule::default()),
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
    plugin.push_top(Record::new(T(b"DIAL"), TOPIC).zsub(T(b"EDID"), "PatrolTopic"));
    plugin.push_top(
        Record::new(T(b"FURN"), FURNITURE)
            .zsub(T(b"EDID"), "PatrolFurniture")
            .sub(T(b"MNAM"), 1u32.to_le_bytes())
            .sub(T(b"FNPR"), [1_u8, 0, 0, 0]),
    );
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
    let first = placed(*b"REFR", A, FURNITURE, 110.0)
        .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
        .sub(T(b"XPPA"), [])
        .sub(T(b"INAM"), 0u32.to_le_bytes())
        .sub(T(b"PDTO"), ptdo())
        .sub(T(b"XLKR"), B.to_le_bytes());
    plugin.push_top(first);
    plugin.push_top(
        placed(*b"REFR", B, 0, 220.0)
            .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
            .sub(T(b"XPPA"), [])
            .sub(T(b"INAM"), 0u32.to_le_bytes()),
    );
    let mut load_order = LoadOrder::new();
    load_order
        .add(
            "Ai88FurnitureSave.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    load_order
}

fn game() -> Game {
    let mut game = Game::new(order());
    game.player();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_88_saved_furniture_reservation_releases_after_dwell() {
    let mut original = game();
    original.tick(1.0);
    assert_eq!(original.world().furniture_seat(id(ACTOR)), Some((id(A), 0)));
    assert_eq!(
        original
            .world()
            .dialogue_session()
            .and_then(|session| session.topic),
        Some(id(TOPIC))
    );
    original.close_dialogue();

    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order());
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world().furniture_seat(id(ACTOR)), Some((id(A), 0)));

    restored.tick(0.5);
    assert_eq!(restored.world().furniture_seat(id(ACTOR)), Some((id(A), 0)));
    restored.tick(0.5);
    assert_eq!(
        restored
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .current,
        id(B)
    );
    assert_eq!(restored.world().furniture_seat(id(ACTOR)), None);
}
