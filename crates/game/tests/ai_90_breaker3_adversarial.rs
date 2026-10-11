// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Patrol omitted optional inputs use their false defaults.

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

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn marker(form: u32, x: f32, next: Option<u32>) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    let mut record = Record::new(T(b"REFR"), form)
        .sub(T(b"NAME"), 0u32.to_le_bytes())
        .sub(T(b"DATA"), data)
        .sub(T(b"XPRD"), 0.0f32.to_le_bytes())
        .sub(T(b"XPPA"), [])
        .sub(T(b"INAM"), 0u32.to_le_bytes());
    if let Some(next) = next {
        record = record.sub(T(b"XLKR"), next.to_le_bytes());
    }
    record
}

fn game_with_omitted_repeatable() -> Game {
    let template = package::close_header(package::build_template(TEMPLATE, "Patrol"), None)
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(
            T(b"PTDA"),
            [
                0i32.to_le_bytes().as_slice(),
                &A.to_le_bytes(),
                &0i32.to_le_bytes(),
            ]
            .concat(),
        )
        .sub(T(b"UNAM"), [7]);
    let template = package::with_float_input(template, 11, 0.0);
    let template = package::with_bool_input(template, 3, true);
    let template = package::with_bool_input(template, 9, false);
    let template = package::with_bool_input(template, 14, false);
    let template = package::with_bool_input(template, 15, false);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Patrol", &[7, 11, u8::MAX, 9, 14, 15], false);

    let mut plugin = Plugin::new(FileHeader::default());
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
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24])
            .sub(T(b"XLKR"), A.to_le_bytes()),
    );
    plugin.push_top(marker(A, 110.0, Some(B)));
    plugin.push_top(marker(B, 220.0, None));
    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_90_omitted_repeatable_does_not_inherit_true_template_value() {
    let mut game = game_with_omitted_repeatable();
    for expected in [110.0, 110.0, 220.0, 220.0] {
        game.tick(1.0);
        assert_eq!(
            game.world_mut().reference(id(ACTOR)).unwrap().position[0],
            expected
        );
    }
    let actor = game.world().actor_ref(id(ACTOR)).unwrap();
    assert!(actor.ai.completed.contains_key(&id(PACK)));
}
