// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Escort validates its target, walks to its authored location, and completes.

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const TARGET: u32 = 0x904;
const DESTINATION: u32 = 0x905;
const CELL: u32 = 0x910;

fn placed(tag: [u8; 4], id: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::new();
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&tag), id)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn target_input(record: Record, index: u8, target: u32) -> Record {
    let mut data = 0i32.to_le_bytes().to_vec();
    data.extend_from_slice(&target.to_le_bytes());
    data.extend_from_slice(&80i32.to_le_bytes());
    record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [index])
}

fn game(include_target: bool, include_location: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut template =
        package::close_header(package::build_template(TEMPLATE, "EscortTemplate"), None);
    template = target_input(template, 0, TARGET);
    if include_location {
        template =
            package::with_location_input(template, 1, Location::NearReference(DESTINATION), 0);
    }
    template = package::finish(template);
    let template = package::with_procedure(template, "Escort", &[0, 1], false);
    plugin.push_top(package::with_actions(template));
    let package_record = package::close_header(
        package::build_record(PACK, "EscortPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "EscortActor",
            "Escort actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );

    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    let mut records = vec![
        placed(*b"ACHR", ACTOR, BASE, [40.0, 100.0, 0.0]),
        placed(*b"REFR", DESTINATION, BASE, [340.0, 100.0, 0.0]),
    ];
    if include_target {
        records.push(placed(*b"ACHR", TARGET, BASE, [80.0, 100.0, 0.0]));
    }
    for record in records {
        let index = plugin.push_record(record);
        temporary.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell));
    block.entries.push(Entry::Group(children));
    let mut interior = Group::new(GroupKind::InteriorBlock(0));
    interior.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(interior));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order
        .add("Escort.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

#[test]
fn ai_26_escort_routes_to_authored_destination_and_completes_on_arrival() {
    let mut game = game(true, true);
    for _ in 0..40 {
        game.tick(0.25);
        let mut world = game.world_mut();
        let actor = world.reference(id(ACTOR)).unwrap().position;
        world.reference(id(TARGET)).unwrap().position = [actor[0] - 20.0, actor[1], actor[2]];
    }
    let destination = {
        let mut world = game.world_mut();
        let ai = world.actor_ref(id(ACTOR)).unwrap().ai.clone();
        let position = world.reference(id(ACTOR)).unwrap().position;
        let destination = world.reference(id(DESTINATION)).unwrap().position;
        assert_eq!(ai.procedure, Procedure::Escort);
        assert!(ai.arrived, "escort procedure did not arrive");
        assert!(!ai.tree.as_ref().is_some_and(|tree| tree.finished));
        assert!((position[0] - destination[0]).hypot(position[1] - destination[1]) <= 4.0);
        world.reference(id(TARGET)).unwrap().position = destination;
        destination
    };
    game.tick(0.1);
    let mut world = game.world_mut();
    let ai = world.actor_ref(id(ACTOR)).unwrap().ai.clone();
    let position = world.reference(id(ACTOR)).unwrap().position;
    assert!(ai.tree.as_ref().is_some_and(|tree| tree.finished));
    assert!(ai.completed.contains_key(&id(PACK)));
    assert!((position[0] - destination[0]).hypot(position[1] - destination[1]) <= 4.0);
}

#[test]
fn ai_26_escort_rejects_a_missing_assigned_target() {
    let mut game = game(false, true);
    for _ in 0..3 {
        game.tick(0.1);
    }
    assert!(matches!(
        game.world_mut().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported(_)
    ));
}

#[test]
fn ai_26_escort_rejects_a_missing_authored_destination() {
    let mut game = game(true, false);
    for _ in 0..3 {
        game.tick(0.1);
    }
    assert!(matches!(
        game.world_mut().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported(_)
    ));
}

#[test]
fn ai_26_escort_waits_for_a_distant_target_before_advancing() {
    let mut game = game(true, true);
    let initial = {
        let mut world = game.world_mut();
        world.reference(id(TARGET)).unwrap().position = [-120.0, 100.0, 0.0];
        world.reference(id(ACTOR)).unwrap().position
    };
    for _ in 0..4 {
        game.tick(0.25);
    }
    {
        let mut world = game.world_mut();
        assert_eq!(world.reference(id(ACTOR)).unwrap().position, initial);
        world.reference(id(TARGET)).unwrap().position = [200.0, 100.0, 0.0];
    }
    game.tick(0.25);
    assert_ne!(
        game.world_mut().reference(id(ACTOR)).unwrap().position,
        initial
    );
}

#[test]
fn ai_26_escort_round_trips_and_rejects_unknown_sections_and_future_world_versions() {
    let mut running = game(true, true);
    running.tick(0.2);
    let bytes = running.world().save_state();
    assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 37);
    let restored = game(true, true);
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .procedure,
        Procedure::Escort
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
    assert!(restored
        .world_mut()
        .load_state(&future)
        .unwrap_err()
        .to_string()
        .contains("world state version"));
}
