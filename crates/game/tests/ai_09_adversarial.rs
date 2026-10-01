// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::navm::{DoorLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ai, Game};

const CELL: u32 = 0x800;
const BASE: u32 = 0x801;
const ACTOR: u32 = 0x802;
const DOOR_BASE: u32 = 0x803;
const DOOR: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn navmesh() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
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
        door_links: vec![DoorLink {
            triangle: 0,
            crc: 0,
            door: DOOR,
        }],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [10.0, 10.0],
            min: [0.0, 0.0, 0.0],
            max: [10.0, 10.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn reference(object: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0; 12]);
    let mut record = Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(Tag(*b"DATA"), data);
    record.placement.cell = Some(CELL);
    record
}

fn game(door_position: [f32; 3]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    let mut actor = Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), BASE.to_le_bytes());
    actor.placement.cell = Some(CELL);
    plugin.push_top(actor);
    plugin.push_top(Record::new(Tag(*b"DOOR"), DOOR_BASE));
    plugin.push_top(reference(DOOR, DOOR_BASE, door_position));
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x805).sub(Tag(*b"NVNM"), navmesh().to_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai09Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_09_moving_closed_door_outside_its_linked_triangle_unblocks_and_invalidates_route() {
    let mut game = game([7.0, 2.0, 0.0]);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.0);
    {
        let mut world = game.world_mut();
        let graph = ai::nav_graph(&mut world);
        assert!(graph
            .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
            .is_none());
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.ai.path = vec![[4.0, 4.0, 0.0]];
        actor.ai.path_cells = vec![Some(id(CELL))];
        world.reference(id(DOOR)).unwrap().position = [20.0, 20.0, 0.0];
    }

    game.tick(0.0);

    let mut world = game.world_mut();
    let graph = ai::nav_graph(&mut world);
    assert!(graph
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_some());
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.ai.path.is_empty());
    assert!(actor.ai.path_cells.is_empty());
}

#[test]
fn ai_09_unresolved_navmesh_door_reference_does_not_close_its_triangle() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    let mut mesh = navmesh();
    mesh.door_links[0].door = 0x900;
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x805).sub(Tag(*b"NVNM"), mesh.to_bytes()));
    let mut order = LoadOrder::new();
    order.add("MissingDoor.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut world = game.world_mut();
    let graph = ai::nav_graph(&mut world);
    let door_key = (u64::from(id(0x900).file) << 32) | u64::from(id(0x900).object);
    assert_eq!(graph.door_nodes(door_key).len(), 1);
    assert!(graph
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_some());
}
