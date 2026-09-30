// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn route_game(target_kind: i32, target: u32) -> Game {
    let template = 0x800;
    let pack = 0x801;
    let base = 0x802;
    let actor = 0x803;
    let marker_base = 0x804;
    let marker = 0x805;
    let mut plugin = Plugin::new(FileHeader::default());
    let mut target_data = target_kind.to_le_bytes().to_vec();
    target_data.extend_from_slice(&target.to_le_bytes());
    target_data.extend_from_slice(&0i32.to_le_bytes());
    let procedure = package::close_header(package::build_template(template, "TargetModes"), None)
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), target_data)
        .sub(T(b"UNAM"), [7]);
    let procedure = package::with_float_input(procedure, 11, 0.0);
    let procedure = package::with_bool_input(procedure, 3, false);
    let procedure = package::with_bool_input(procedure, 9, false);
    let procedure = package::with_bool_input(procedure, 14, false);
    let procedure = package::finish(package::with_bool_input(procedure, 15, false));
    plugin.push_top(package::with_procedure(
        procedure,
        "Patrol",
        &[7, 11, 3, 9, 14, 15],
        false,
    ));
    plugin.push_top(package::finish(package::close_header(
        package::build_record(pack, "Patrol", 0, Schedule::default()),
        Some(template),
    )));
    plugin.push_top(
        npc::build_record(
            base,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), pack.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), actor)
            .sub(T(b"NAME"), base.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    plugin.push_top(Record::new(T(b"MISC"), marker_base).zsub(T(b"EDID"), "RouteMarkerBase"));
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&10.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"REFR"), marker)
            .sub(T(b"NAME"), marker_base.to_le_bytes())
            .sub(T(b"DATA"), data)
            .sub(T(b"XPRD"), 0.0f32.to_le_bytes())
            .sub(T(b"XPPA"), []),
    );
    let mut order = LoadOrder::new();
    order.add("PatrolTargets.esp", plugin).unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(actor)));
    game
}

#[test]
fn ai_91_object_id_path_start_selects_route_reference_by_base_record() {
    let mut game = route_game(1, 0x804);
    game.tick(0.1);
    let world = game.world();
    let ai = &world.actor_ref(id(0x803)).unwrap().ai;
    assert_eq!(ai.patrol.as_ref().map(|p| p.start), Some(id(0x805)));
}

#[test]
fn ai_91_object_type_search_start_uses_the_authored_target_namespace() {
    let mut game = route_game(2, 8); // xEdit and UESP identify 8 as Miscellaneous.
    game.tick(0.1);
    let world = game.world();
    let ai = &world.actor_ref(id(0x803)).unwrap().ai;
    assert_eq!(ai.patrol.as_ref().map(|p| p.start), Some(id(0x805)));
}
