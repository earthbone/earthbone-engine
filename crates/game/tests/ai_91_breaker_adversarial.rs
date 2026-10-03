// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACKAGE: u32 = 0x801;
const ACTOR_BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const ARMOR: u32 = 0x804;
const MARKER: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(object_type: u32, armor_type: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut target = 2u32.to_le_bytes().to_vec();
    target.extend_from_slice(&object_type.to_le_bytes());
    target.extend_from_slice(&0i32.to_le_bytes());
    let template = package::close_header(package::build_template(TEMPLATE, "Patrol"), None)
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), target)
        .sub(T(b"UNAM"), [7]);
    let template = package::with_float_input(template, 11, 0.0);
    let template = package::with_bool_input(template, 3, false);
    let template = package::with_bool_input(template, 9, false);
    let template = package::with_bool_input(template, 14, false);
    let template = package::finish(package::with_bool_input(template, 15, false));
    plugin.push_top(package::with_procedure(
        template,
        "Patrol",
        &[7, 11, 3, 9, 14, 15],
        false,
    ));
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACKAGE, "Patrol", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    plugin.push_top(
        npc::build_record(
            ACTOR_BASE,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    let mut body = vec![0; 8];
    body[4..8].copy_from_slice(&armor_type.to_le_bytes());
    plugin.push_top(Record::new(T(b"ARMO"), ARMOR).sub(T(b"BOD2"), body));
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&10.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"REFR"), MARKER)
            .sub(T(b"NAME"), ARMOR.to_le_bytes())
            .sub(T(b"DATA"), data)
            .sub(T(b"XPRD"), 0.0f32.to_le_bytes())
            .sub(T(b"XPPA"), []),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "ArmorPatrol.esp",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn object_type_16_matches_light_and_heavy_bod2_values() {
    for armor_type in [0, 1] {
        let mut game = game(16, armor_type);
        game.tick(0.1);
        assert_eq!(
            game.world()
                .actor_ref(id(ACTOR))
                .unwrap()
                .ai
                .patrol
                .as_ref()
                .map(|patrol| patrol.start),
            Some(id(MARKER)),
            "BOD2 armor type {armor_type}"
        );
    }
}

#[test]
fn object_type_16_rejects_clothing_while_object_type_17_accepts_it() {
    let mut combat_wearable = game(16, 2);
    combat_wearable.tick(0.1);
    assert!(combat_wearable
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .patrol
        .is_none());

    let mut wearable = game(17, 2);
    wearable.tick(0.1);
    assert_eq!(
        wearable
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .map(|patrol| patrol.start),
        Some(id(MARKER))
    );
}
