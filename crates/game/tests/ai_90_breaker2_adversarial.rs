// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Patrol edge cases for absent PKC2 targets and all optional defaults.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
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

fn patrol(missing_target_input: bool, omit_optionals: bool) -> Game {
    let mut template = package::close_header(package::build_template(TEMPLATE, "Patrol"), None)
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
    template = package::with_float_input(template, 11, 0.0);
    template = package::with_bool_input(template, 3, false);
    template = package::with_bool_input(template, 9, false);
    template = package::with_bool_input(template, 14, false);
    template = package::with_bool_input(template, 15, false);
    let template = package::finish(template);
    let inputs = if missing_target_input {
        [99, 11, 3, 9, 14, 15]
    } else if omit_optionals {
        [7, 11, u8::MAX, u8::MAX, u8::MAX, u8::MAX]
    } else {
        [7, 11, 3, 9, 14, 15]
    };
    let template = package::with_procedure(template, "Patrol", &inputs, false);

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
fn ai_90_non_sentinel_input_index_without_data_is_unsupported_before_movement() {
    let mut game = patrol(true, false);
    let tick = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| game.tick(1.0)));
    assert!(
        tick.is_ok(),
        "malformed PKC2 input index panicked during tick"
    );
    let mut world = game.world_mut();
    let actor = world.reference(id(ACTOR)).unwrap();
    assert_eq!(actor.position[0], 0.0);
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(matches!(ai.procedure, Procedure::Unsupported(_)));
    assert!(ai.completed.is_empty());
}

#[test]
fn ai_90_all_omitted_optionals_keep_finite_route_and_complete() {
    let mut game = patrol(false, true);
    for expected in [110.0, 110.0, 220.0, 220.0] {
        game.tick(1.0);
        assert_eq!(
            game.world_mut().reference(id(ACTOR)).unwrap().position[0],
            expected
        );
    }
    game.tick(1.0);
    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert!(actor.ai.completed.contains_key(&id(PACK)));
    assert!(!matches!(actor.ai.procedure, Procedure::Unsupported(_)));
}
