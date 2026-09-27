// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x803;
const RIDER_BASE: u32 = 0x802;
const HORSE: u32 = 0x808;
const A: u32 = 0x804;
const B: u32 = 0x805;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn place(tag: [u8; 4], form: u32, base: u32) -> Record {
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), [0; 24])
}

#[test]
fn ai_89_unconscious_owned_horse_is_not_selected() {
    let mut template = package::close_header(package::build_template(0x800, "Mounted"), None)
        .zsub(T(b"ANAM"), "SingleRef");
    let mut target = 0_i32.to_le_bytes().to_vec();
    target.extend_from_slice(&A.to_le_bytes());
    target.extend_from_slice(&0_i32.to_le_bytes());
    template = template.sub(T(b"PTDA"), target).sub(T(b"UNAM"), [7]);
    template = package::with_float_input(template, 11, 0.0);
    template = package::with_bool_input(template, 3, false);
    template = package::with_bool_input(template, 9, false);
    template = package::with_bool_input(template, 14, true);
    template = package::finish(package::with_bool_input(template, 15, false));
    let template = package::with_procedure(template, "Patrol", &[7, 11, 3, 9, 14, 15], false);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template);
    plugin.push_top(package::finish(package::close_header(package::build_record(0x801, "Mounted", 0, Schedule::default()), Some(0x800))));
    plugin.push_top(npc::build_record(RIDER_BASE, "Rider", "Rider", Config::default(), Stats::default()).sub(T(b"PKID"), 0x801u32.to_le_bytes()));
    plugin.push_top(npc::build_record(0x807, "Horse", "Horse", Config::default(), Stats::default()).sub(T(b"RNAM"), 0x809u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"RACE"), 0x809).zsub(T(b"EDID"), "HorseRace").sub(T(b"KWDA"), 0x80bu32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"KYWD"), 0x80b).zsub(T(b"EDID"), "ActorTypeHorse"));
    plugin.push_top(place(*b"ACHR", ACTOR, RIDER_BASE).sub(T(b"XLKR"), A.to_le_bytes()));
    let a = place(*b"REFR", A, 0).sub(T(b"XPRD"), 0.0f32.to_le_bytes()).sub(T(b"XLKR"), B.to_le_bytes());
    let b = place(*b"REFR", B, 0).sub(T(b"XPRD"), 0.0f32.to_le_bytes());
    plugin.push_top(a);
    plugin.push_top(b);
    plugin.push_top(place(*b"ACHR", HORSE, 0x807).sub(T(b"XOWN"), RIDER_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("UnconsciousHorse.esm", Plugin::parse(&plugin.to_bytes()).unwrap()).unwrap();
    let mut game = Game::new(order);
    game.world_mut().actor(id(HORSE)).unwrap().unconscious = true;
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.0);
    assert_eq!(game.world().actor_ref(id(ACTOR)).unwrap().ai.patrol.as_ref().unwrap().mount, None);
}
