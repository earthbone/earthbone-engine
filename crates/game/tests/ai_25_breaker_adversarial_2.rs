// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
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
const CELL: u32 = 0x910;
const NAV: u32 = 0x911;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::new();
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn hole_mesh(cell: u32) -> Geometry {
    let stride = 13;
    let mut vertices = Vec::new();
    for y in 0..=12 {
        for x in 0..=12 {
            vertices.push([x as f32 * 25.0, y as f32 * 25.0, 0.0]);
        }
    }
    let mut triangles = Vec::new();
    for y in 0..12 {
        for x in 0..12 {
            if (4..8).contains(&x) && (4..8).contains(&y) {
                continue;
            }
            let bl = y * stride + x;
            let br = bl + 1;
            let tl = bl + stride;
            let tr = tl + 1;
            triangles.push(Triangle {
                vertices: [bl as u16, br as u16, tr as u16],
                edges: [-1; 3],
                flags: 0,
                cover_flags: 0,
            });
            triangles.push(Triangle {
                vertices: [bl as u16, tr as u16, tl as u16],
                edges: [-1; 3],
                flags: 0,
                cover_flags: 0,
            });
        }
    }
    for first in 0..triangles.len() {
        for second in 0..triangles.len() {
            if first == second {
                continue;
            }
            for edge in 0..3 {
                let a = triangles[first].vertices[edge];
                let b = triangles[first].vertices[(edge + 1) % 3];
                for other_edge in 0..3 {
                    if triangles[second].vertices[other_edge] == b
                        && triangles[second].vertices[(other_edge + 1) % 3] == a
                    {
                        triangles[first].edges[edge] = second as i16;
                    }
                }
            }
        }
    }
    let triangle_count = triangles.len() as i16;
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(cell),
        vertices,
        triangles,
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [300.0, 300.0],
            min: [0.0, 0.0, 0.0],
            max: [300.0, 300.0, 0.0],
            cells: vec![(0..triangle_count).collect()],
        },
    }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "OrbitTemplate"), None);
    let template = package::with_location_input(template, 0, Location::NearReference(TARGET), 0);
    let template = package::with_float_input(template, 1, 70.75);
    let template = package::with_float_input(template, 2, 70.75);
    let template = package::with_float_input(template, 3, 0.0);
    let template = package::finish(template);
    plugin.push_top(package::with_actions(package::with_procedure(
        template,
        "Orbit",
        &[0, 1, 2, 3],
        false,
    )));
    let package = package::close_header(
        package::build_record(PACK, "OrbitPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(package)));
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
        placed(*b"ACHR", ACTOR, BASE, [200.03, 200.03, 0.0]),
        placed(*b"REFR", TARGET, BASE, [150.0, 150.0, 0.0]),
        Record::new(T(b"NAVM"), NAV).sub(T(b"NVNM"), hole_mesh(CELL).to_bytes()),
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
        .add("OrbitGap.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

#[test]
fn ai_25_high_speed_orbit_tracks_its_radius_around_a_navmesh_hole() {
    let mut game = game();
    game.world_mut().actor(id(ACTOR)).unwrap().ai.speed = 1000.0;
    game.world_mut().reference(id(ACTOR)).unwrap().position = [200.03, 200.03, 0.0];
    let start = game.world_mut().reference(id(ACTOR)).unwrap().position;
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Orbit { height: 0.0 }
    );
    let route_end = [
        150.0 + 70.75 * 0.6f32.cos(),
        150.0 + 70.75 * 0.6f32.sin(),
        0.0,
    ];
    assert!(soe_game::ai::nav_graph(&mut game.world_mut())
        .find_path_between(start, route_end)
        .is_some());
    for _ in 0..30 {
        game.tick(0.1);
        let mut world = game.world_mut();
        let position = world.reference(id(ACTOR)).unwrap().position;
        let radius = (position[0] - 150.0).hypot(position[1] - 150.0);
        assert!(
            (70.7..=70.8).contains(&radius),
            "Orbit left its authored radius: {radius}"
        );
        assert!(soe_game::ai::nav_graph(&mut world)
            .locate(position)
            .is_some());
    }
    let mut world = game.world_mut();
    let end = world.reference(id(ACTOR)).unwrap().position;
    let radius = (end[0] - 150.0).hypot(end[1] - 150.0);
    assert!(start != end, "Orbit stalled at the navmesh-hole corner");
    assert!(
        (70.7..=70.8).contains(&radius),
        "Orbit left its authored radius: {radius}"
    );
    assert!(soe_game::ai::nav_graph(&mut world).locate(end).is_some());
}

#[test]
fn ai_25_repeated_zero_time_calls_keep_the_actor_on_navmesh() {
    let mut game = game();
    let start = game.world_mut().reference(id(ACTOR)).unwrap().position;
    for _ in 0..20 {
        game.tick(0.0);
    }
    let mut world = game.world_mut();
    let position = world.reference(id(ACTOR)).unwrap().position;
    assert_eq!(position, start);
    assert!(soe_game::ai::nav_graph(&mut world)
        .locate(position)
        .is_some());
}
