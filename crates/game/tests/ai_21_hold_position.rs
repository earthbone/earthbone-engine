// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HoldPosition keeps an actor near its package branch location until the
//! package stops winning.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACKAGE: u32 = 0x901;
const BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const TARGET: u32 = 0x904;
const ENABLED: u32 = 0x905;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "HoldPosition"), None);
    let template = package::with_location_input(template, 0, Location::NearReference(TARGET), 0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "HoldPosition", &[0], false);
    let template = package::with_input_name(template, 0, "Hold Location", true);
    plugin.push_top(package::with_actions(template));

    let enabled = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: ENABLED,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let package_record = package::close_header(
        package::build_record(PACKAGE, "HoldPositionPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let package_record =
        package::with_location_input(package_record, 0, Location::NearReference(TARGET), 0);
    let package_record = package::with_condition(package_record, &enabled);
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Holder",
            "Holder",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, [0.0, 0.0, 0.0]));
    plugin.push_top(placed(*b"REFR", TARGET, 0, [40.0, 0.0, 0.0]));
    plugin.push_top(
        Record::new(T(b"GLOB"), ENABLED)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );

    let mut order = LoadOrder::new();
    order
        .add(
            "HoldPosition.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    order
}

fn game() -> Game {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(1.0);
    game
}

#[test]
fn ai_21_hold_position_returns_to_the_authored_place_within_tolerance() {
    let mut game = game();
    let procedure = game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .procedure
        .clone();
    let position = game.world_mut().reference(id(ACTOR)).unwrap().position;
    assert_eq!(procedure, Procedure::HoldPosition);
    assert!(
        (position[0] - 40.0).abs() <= 4.0,
        "actor stopped outside the authored hold tolerance at {position:?}"
    );

    game.world_mut().reference(id(ACTOR)).unwrap().position = [80.0, 0.0, 0.0];
    for _ in 0..5 {
        game.tick(0.1);
    }
    let procedure = game
        .world()
        .actor_ref(id(ACTOR))
        .unwrap()
        .ai
        .procedure
        .clone();
    let position = game.world_mut().reference(id(ACTOR)).unwrap().position;
    assert_eq!(procedure, Procedure::HoldPosition);
    assert!(
        (position[0] - 40.0).abs() <= 4.0,
        "actor did not return within four units of the authored place: {position:?}"
    );
}

#[test]
fn ai_21_hold_position_stays_active_until_its_package_condition_ends() {
    let mut game = game();
    game.world_mut().reference(id(ACTOR)).unwrap().position = [80.0, 0.0, 0.0];
    game.world_mut().set_global(id(ENABLED), 0.0);
    game.tick(1.1);

    let (package, procedure) = {
        let world = game.world();
        let actor = world.actor_ref(id(ACTOR)).unwrap();
        (actor.ai.package, actor.ai.procedure.clone())
    };
    let position = game.world_mut().reference(id(ACTOR)).unwrap().position;
    assert_eq!(package, None);
    assert_eq!(procedure, Procedure::None);
    assert_eq!(position, [80.0, 0.0, 0.0]);
}

#[test]
fn ai_21_hold_position_save_roundtrip_checks_sections_and_future_versions() {
    let game = game();
    let bytes = game.world().save_state();
    assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 37);
    let restored = Game::new(order());
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::HoldPosition
    );

    let mut unknown_section = bytes.clone();
    let section = unknown_section
        .windows(5)
        .position(|name| name == b"AI-21")
        .unwrap();
    unknown_section[section..section + 5].copy_from_slice(b"AI-00");
    assert!(restored
        .world_mut()
        .load_state(&unknown_section)
        .unwrap_err()
        .to_string()
        .contains("unknown section AI-00 at actor"));

    let mut future = bytes;
    future[4..6].copy_from_slice(&(soe_game::world::SAVE_VERSION + 1).to_le_bytes());
    assert!(restored.world_mut().load_state(&future).is_err());
}
