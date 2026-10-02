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
const ARMOR_NONCOMBAT: u32 = 0x804;
const ARMOR_COMBAT: u32 = 0x805;
const FIRST: u32 = 0x806;
const SECOND: u32 = 0x807;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn armor(form: u32, armor_type: u32) -> Record {
    let mut body = vec![0; 16];
    body[12..16].copy_from_slice(&armor_type.to_le_bytes());
    Record::new(T(b"ARMO"), form).sub(T(b"BOD2"), body)
}

fn placed(form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(b"REFR"), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
        .sub(T(b"XPRD"), 0.0f32.to_le_bytes())
        .sub(T(b"XPPA"), [])
}

#[test]
fn ai_91_object_type_sixteen_skips_noncombat_armor_and_saves_selected_reference() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut target = 2i32.to_le_bytes().to_vec();
    target.extend_from_slice(&16u32.to_le_bytes());
    target.extend_from_slice(&0i32.to_le_bytes());
    let template = package::close_header(package::build_template(TEMPLATE, "CombatArmorPatrol"), None)
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
        package::build_record(PACK, "Patrol", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    plugin.push_top(
        npc::build_record(BASE, "Walker", "Walker", Config::default(), Stats::default())
            .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    plugin.push_top(armor(ARMOR_NONCOMBAT, 0));
    plugin.push_top(armor(ARMOR_COMBAT, 1));
    plugin.push_top(placed(FIRST, ARMOR_NONCOMBAT, 10.0));
    plugin.push_top(placed(SECOND, ARMOR_COMBAT, 20.0));
    let mut order = LoadOrder::new();
    order
        .add("CombatArmorPatrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    let mut game = game;
    game.tick(0.1);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .start,
        id(SECOND)
    );
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .patrol
            .as_ref()
            .unwrap()
            .start,
        id(SECOND)
    );
}
