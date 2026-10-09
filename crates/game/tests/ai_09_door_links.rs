// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-09 runtime NAVM door binding and path invalidation.

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

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(Record::new(Tag(*b"DOOR"), DOOR_BASE));
    plugin.push_top(reference(DOOR, DOOR_BASE, [7.0, 2.0, 0.0]));
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x805).sub(Tag(*b"NVNM"), navmesh().to_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai09.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prf_11_unchanged_doors_preserve_routes_and_live_changes_invalidate_them() {
    let game = game();
    let mut world = game.world_mut();
    world.actor(id(ACTOR)).unwrap().dead = true;
    ai::nav_graph(&mut world);
    let revision = world.ai.nav_graph().unwrap().query_revision();
    world.actor(id(ACTOR)).unwrap().ai.path = vec![[4.0, 4.0, 0.0]];
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    for _ in 0..60 {
        ai::tick(&mut world, 0.0, &mut alloc);
        assert_eq!(world.ai.nav_graph().unwrap().query_revision(), revision);
        assert_eq!(
            world.actor_ref(id(ACTOR)).unwrap().ai.path,
            vec![[4.0, 4.0, 0.0]]
        );
    }
    world.reference(id(DOOR)).unwrap().opened = true;
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(
        world.actor_ref(id(ACTOR)).unwrap().ai.path.as_slice(),
        &[] as &[[f32; 3]]
    );
    assert!(world.ai.nav_graph().unwrap().query_revision() > revision);
    world.reference(id(DOOR)).unwrap().opened = false;
    ai::tick(&mut world, 0.0, &mut alloc);
    let revision = world.ai.nav_graph().unwrap().query_revision();
    world.actor(id(ACTOR)).unwrap().ai.path = vec![[4.0, 4.0, 0.0]];
    world.reference(id(DOOR)).unwrap().position = [20.0, 20.0, 0.0];
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(
        world.actor_ref(id(ACTOR)).unwrap().ai.path.as_slice(),
        &[] as &[[f32; 3]]
    );
    assert!(world.ai.nav_graph().unwrap().query_revision() > revision);
    assert!(world
        .ai
        .nav_graph()
        .unwrap()
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_some());
}

#[test]
fn prf_36_bound_doors_are_checked_only_after_mutation_and_restore() {
    let game = game();
    let mut world = game.world_mut();
    world.actor(id(ACTOR)).unwrap().dead = true;
    ai::nav_graph(&mut world);
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    for _ in 0..8 {
        ai::tick(&mut world, 0.0, &mut alloc);
        assert_eq!(world.ai.door_checks, 0);
    }
    let revision = world.ai.nav_graph().unwrap().query_revision();
    assert!(!world.reference(id(DOOR)).unwrap().opened);
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(world.ai.door_checks, 1);
    assert_eq!(world.ai.nav_graph().unwrap().query_revision(), revision);
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(world.ai.door_checks, 0);
    let closed = world.save_state();
    world.reference(id(DOOR)).unwrap().opened = true;
    ai::tick(&mut world, 0.0, &mut alloc);
    assert!(world.ai.nav_graph().unwrap().query_revision() > revision);
    world.load_state(&closed).unwrap();
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(world.ai.door_checks, 1);
    assert!(world
        .ai
        .nav_graph()
        .unwrap()
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_none());
}

#[test]
fn prf_36_translated_and_reset_doors_refresh_their_navmesh_footprint() {
    let game = game();
    let mut world = game.world_mut();
    world.actor(id(ACTOR)).unwrap().dead = true;
    ai::nav_graph(&mut world);
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    world.start_translation(
        id(DOOR),
        [20.0, 20.0, 0.0],
        [0.0; 3],
        1.0,
        0.0,
        soe_game::world::TranslationPath::Linear,
        &mut alloc,
    );
    ai::tick(&mut world, 0.0, &mut alloc);
    let revision = world.ai.nav_graph().unwrap().query_revision();
    world.tick_translations(100.0, &mut alloc);
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(world.ai.door_checks, 1);
    assert!(world.ai.nav_graph().unwrap().query_revision() > revision);
    assert!(world
        .ai
        .nav_graph()
        .unwrap()
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_some());
    assert!(world.reset_reference(id(DOOR), None));
    ai::tick(&mut world, 0.0, &mut alloc);
    assert_eq!(world.ai.door_checks, 1);
    assert!(world
        .ai
        .nav_graph()
        .unwrap()
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_none());
}

trait WithCell {
    fn with_cell(self, cell: u32) -> Self;
}

impl WithCell for Record {
    fn with_cell(mut self, cell: u32) -> Self {
        self.placement.cell = Some(cell);
        self
    }
}

#[test]
fn ai_09_closed_door_blocks_its_triangle_and_opening_invalidates_cached_path() {
    let mut game = game();
    {
        let mut world = game.world_mut();
        let graph = ai::nav_graph(&mut world);
        let door_key = (u64::from(id(DOOR).file) << 32) | u64::from(id(DOOR).object);
        assert_eq!(graph.door_nodes(door_key).len(), 1);
        assert!(graph
            .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
            .is_none());
        let actor = world.actor(id(ACTOR)).expect("actor reference");
        actor.ai.path = vec![[4.0, 4.0, 0.0]];
        actor.ai.path_cells = vec![Some(id(CELL))];
        world.reference(id(DOOR)).expect("door reference").opened = true;
    }

    game.tick(0.0);

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).expect("actor reference");
    assert!(
        actor.ai.path.is_empty(),
        "door state change must invalidate route"
    );
    assert_eq!(
        actor.ai.path_cells.as_slice(),
        &[] as &[Option<soe_esm::GlobalFormId>]
    );
    let graph = ai::nav_graph(&mut world);
    assert!(graph
        .find_path_between([7.0, 2.0, 0.0], [2.0, 8.0, 0.0])
        .is_some());
}
