// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Orbit follows its resolved location and authored bounds on the navmesh.

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const TARGET: u32 = 0x904;
const CELL: u32 = 0x910;
const NAV: u32 = 0x911;

fn square(cell: u32) -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(cell),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [400.0, 0.0, 0.0],
            [400.0, 400.0, 0.0],
            [0.0, 400.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [400.0, 400.0],
            min: [0.0, 0.0, 0.0],
            max: [400.0, 400.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

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

fn build_game(inner: f32, outer: f32, target: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "OrbitTemplate"), None);
    let template = package::with_location_input(template, 0, Location::NearReference(target), 0);
    let template = package::with_float_input(template, 1, inner);
    let template = package::with_float_input(template, 2, outer);
    let template = package::with_float_input(template, 3, 25.0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Orbit", &[0, 1, 2, 3], false);
    plugin.push_top(package::with_actions(template));
    let package_record = package::close_header(
        package::build_record(PACK, "OrbitPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "OrbitActor",
            "Orbit actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );

    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    for record in [
        placed(*b"ACHR", ACTOR, BASE, [270.0, 200.0, 0.0]),
        placed(*b"REFR", TARGET, BASE, [200.0, 200.0, 0.0]),
        Record::new(T(b"NAVM"), NAV).sub(T(b"NVNM"), square(CELL).to_bytes()),
    ] {
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
        .add("Orbit.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
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
fn ai_25_orbit_moves_around_its_resolved_target_with_authored_bounds_and_height() {
    let mut game = build_game(40.0, 80.0, TARGET);
    game.world_mut().reference(id(TARGET)).unwrap().position = [220.0, 200.0, 0.0];
    for _ in 0..20 {
        game.tick(0.1);
        let mut world = game.world_mut();
        let position = world.reference(id(ACTOR)).unwrap().position;
        let target = world.reference(id(TARGET)).unwrap().position;
        let radius = (position[0] - target[0]).hypot(position[1] - target[1]);
        assert!((40.0..=80.0).contains(&radius), "orbit radius was {radius}");
        assert_eq!(position[2], target[2] + 25.0);
        assert!(
            soe_game::ai::nav_graph(&mut world)
                .locate(position)
                .is_some()
        );
    }
    let (procedure, position, target) = {
        let mut world = game.world_mut();
        (
            world.actor_ref(id(ACTOR)).unwrap().ai.procedure.clone(),
            world.reference(id(ACTOR)).unwrap().position,
            world.reference(id(TARGET)).unwrap().position,
        )
    };
    assert_eq!(procedure, Procedure::Orbit { height: 25.0 });
    assert!((position[0] - 270.0).hypot(position[1] - 200.0) > 1.0);
    assert!((40.0..=80.0).contains(&(position[0] - target[0]).hypot(position[1] - target[1])));
}

#[test]
fn ai_25_orbit_rejects_invalid_authored_radius_bounds() {
    let game = build_game(90.0, 40.0, TARGET);
    let world = game.world_mut();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert!(matches!(actor.ai.procedure, Procedure::Unsupported(_)));
}

#[test]
fn ai_25_orbit_accepts_equal_inner_and_outer_bounds() {
    let mut game = build_game(60.0, 60.0, TARGET);
    for _ in 0..10 {
        game.tick(0.1);
    }
    let mut world = game.world_mut();
    let position = world.reference(id(ACTOR)).unwrap().position;
    let target = world.reference(id(TARGET)).unwrap().position;
    let radius = (position[0] - target[0]).hypot(position[1] - target[1]);
    assert!((radius - 60.0).abs() < 0.01, "orbit radius was {radius}");
    assert!(world.actor_ref(id(ACTOR)).unwrap().ai.destination.is_some());
}

#[test]
fn ai_25_orbit_does_not_move_without_a_resolved_target_location() {
    let mut game = build_game(40.0, 80.0, 0x999);
    let before = game.world_mut().reference(id(ACTOR)).unwrap().position;
    for _ in 0..5 {
        game.tick(0.1);
    }
    let mut world = game.world_mut();
    assert!(matches!(
        world.actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported(_)
    ));
    assert_eq!(world.reference(id(ACTOR)).unwrap().position, before);
}

#[test]
fn ai_25_orbit_round_trips_and_rejects_future_world_versions() {
    let mut game = build_game(40.0, 80.0, TARGET);
    game.tick(0.2);
    let bytes = game.world().save_state();
    let restored = build_game(40.0, 80.0, TARGET);
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .procedure,
        Procedure::Orbit { height: 25.0 }
    );
    let mut future = bytes;
    future[4..6].copy_from_slice(&(soe_game::world::SAVE_VERSION + 1).to_le_bytes());
    assert!(
        restored
            .world_mut()
            .load_state(&future)
            .unwrap_err()
            .to_string()
            .contains("world state version")
    );
}

#[test]
fn ai_25_orbit_re_resolves_a_moving_target_each_tick() {
    let mut game = build_game(40.0, 80.0, TARGET);
    for step in 0..12 {
        let target_x = 200.0 + step as f32 * 2.0;
        game.world_mut().reference(id(TARGET)).unwrap().position = [target_x, 200.0, 0.0];
        game.tick(0.1);
        let mut world = game.world_mut();
        let actor = world.reference(id(ACTOR)).unwrap().position;
        let radius = (actor[0] - target_x).hypot(actor[1] - 200.0);
        assert!(
            (40.0..=80.0).contains(&radius),
            "orbit lost moving target at radius {radius}"
        );
        assert!(soe_game::ai::nav_graph(&mut world).locate(actor).is_some());
    }
}
