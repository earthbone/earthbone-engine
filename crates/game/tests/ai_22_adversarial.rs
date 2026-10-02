// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Follow tracks a package target across linked navmesh cells and honors its
//! authored catch-up and follow radii.

use soe_esm::records::navm::{EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule, Target};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const TARGET_REF: u32 = 0x904;
const CELL_A: u32 = 0x910;
const CELL_B: u32 = 0x911;
const NAV_A: u32 = 0x912;
const NAV_B: u32 = 0x913;

fn square(cell: u32, x: f32, right: Option<u32>, left: Option<u32>) -> Geometry {
    let mut a = Triangle {
        vertices: [0, 1, 2],
        edges: [-1, -1, 1],
        flags: 0,
        cover_flags: 0,
    };
    let mut b = Triangle {
        vertices: [0, 2, 3],
        edges: [0, -1, -1],
        flags: 0,
        cover_flags: 0,
    };
    let mut links = Vec::new();
    if let Some(nav) = right {
        a.edges[1] = links.len() as i16;
        a.flags |= soe_navmesh::vanilla::flags::EDGE_1_2_LINK;
        links.push(EdgeLink {
            kind: 2,
            navmesh: nav,
            triangle: 1,
        });
    }
    if let Some(nav) = left {
        b.edges[2] = links.len() as i16;
        b.flags |= soe_navmesh::vanilla::flags::EDGE_2_0_LINK;
        links.push(EdgeLink {
            kind: 2,
            navmesh: nav,
            triangle: 0,
        });
    }
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(cell),
        vertices: vec![
            [x, 0.0, 0.0],
            [x + 100.0, 0.0, 0.0],
            [x + 100.0, 100.0, 0.0],
            [x, 100.0, 0.0],
        ],
        triangles: vec![a, b],
        edge_links: links,
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [100.0, 100.0],
            min: [x, 0.0, 0.0],
            max: [x + 100.0, 100.0, 0.0],
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

fn with_target_input(record: Record, index: i8, target: Target) -> Record {
    let (kind, value) = match target {
        Target::Reference(value) => (0i32, value),
        _ => panic!("fixture uses a specific target reference"),
    };
    let mut data = kind.to_le_bytes().to_vec();
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [index as u8])
}

fn cell_group(plugin: &mut Plugin, id: u32, placed: Vec<Record>) -> Group {
    let cell = plugin.push_record(Record::new(T(b"CELL"), id).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(id));
    for record in placed {
        let index = plugin.push_record(record);
        temporary.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(id));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell));
    block.entries.push(Entry::Group(children));
    block
}

fn order(actor_x: f32) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "FollowTemplate"), None);
    let template = with_target_input(template, 0, Target::Reference(TARGET_REF));
    let template = package::with_float_input(template, 1, 50.0);
    let template = package::with_float_input(template, 2, 20.0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Follow", &[0, 1, 2], false);
    let template = package::with_input_name(template, 0, "Target", true);
    let template = package::with_input_name(template, 1, "Catch Up Radius", true);
    let template = package::with_input_name(template, 2, "Follow Radius", true);
    plugin.push_top(package::with_actions(template));
    let package_record = package::close_header(
        package::build_record(PACK, "FollowPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Follower",
            "Follower",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(cell_group(
        &mut plugin,
        CELL_A,
        vec![
            placed(*b"ACHR", ACTOR, BASE, [actor_x, 50.0, 0.0]),
            Record::new(T(b"NAVM"), NAV_A).sub(
                T(b"NVNM"),
                square(CELL_A, 0.0, Some(NAV_B), None).to_bytes(),
            ),
        ],
    )));
    block.entries.push(Entry::Group(cell_group(
        &mut plugin,
        CELL_B,
        vec![
            placed(*b"ACHR", TARGET_REF, BASE, [190.0, 50.0, 0.0]),
            Record::new(T(b"NAVM"), NAV_B).sub(
                T(b"NVNM"),
                square(CELL_B, 100.0, None, Some(NAV_A)).to_bytes(),
            ),
        ],
    )));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add("Follow.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(actor_x: f32) -> Game {
    let mut game = Game::new(order(actor_x));
    assert!(game.world_mut().drive_actor(soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR
    }));
    game.tick(0.1);
    game
}

#[test]
fn ai_22_follow_resolves_target_and_crosses_navmesh_cells() {
    let mut game = game(10.0);
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    };
    let target = soe_esm::GlobalFormId {
        file: 0,
        object: TARGET_REF,
    };
    let cell_b = soe_esm::GlobalFormId {
        file: 0,
        object: CELL_B,
    };
    assert_eq!(
        game.world().actor_ref(actor).unwrap().ai.procedure,
        Procedure::Follow
    );
    for _ in 0..80 {
        game.tick(0.1);
    }
    let (position, cell) = game
        .world_mut()
        .reference(actor)
        .map(|r| (r.position, r.cell))
        .unwrap();
    let target_position = game.world_mut().reference(target).unwrap().position;
    let separation = ((position[0] - target_position[0]).powi(2)
        + (position[1] - target_position[1]).powi(2)
        + (position[2] - target_position[2]).powi(2))
    .sqrt();
    assert!(
        (20.0..=24.0).contains(&separation),
        "follower stopped {separation} units away"
    );
    assert_eq!(
        cell,
        Some(cell_b),
        "follower entered the target's navmesh cell"
    );
}

#[test]
fn ai_22_follow_stays_inside_the_authored_follow_radius() {
    let mut game = game(175.0);
    game.tick(0.2);
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    };
    let x = game.world_mut().reference(actor).unwrap().position[0];
    assert!(
        (x - 175.0).abs() < 0.01,
        "actor already inside the 20 unit follow radius"
    );
}

#[test]
fn ai_22_follow_runs_beyond_the_authored_catch_up_radius() {
    let game = game(10.0);
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    };
    let x = game.world_mut().reference(actor).unwrap().position[0];
    assert!(
        x > 40.0,
        "catch-up movement used run speed, actor reached x={x}"
    );
}

#[test]
fn ai_22_follow_saved_tag_roundtrips_and_rejects_older_world_versions() {
    let game = game(10.0);
    let bytes = game.world().save_state();
    let restored = Game::new(order(10.0));
    restored.world_mut().load_state(&bytes).unwrap();
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    };
    assert_eq!(
        restored.world().actor_ref(actor).unwrap().ai.procedure,
        Procedure::Follow
    );

    let mut older = bytes;
    older[4..6].copy_from_slice(&34u16.to_le_bytes());
    assert!(restored.world_mut().load_state(&older).is_err());
}

#[test]
fn ai_22_follow_retargets_when_target_moves_to_another_navmesh_cell() {
    let mut game = game(10.0);
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    };
    let target = soe_esm::GlobalFormId {
        file: 0,
        object: TARGET_REF,
    };
    game.world_mut().reference(target).unwrap().position = [150.0, 50.0, 0.0];
    for _ in 0..80 {
        game.tick(0.1);
    }
    let mut world = game.world_mut();
    let actor_position = world.reference(actor).unwrap().position;
    let target_position = world.reference(target).unwrap().position;
    let separation =
        (actor_position[0] - target_position[0]).hypot(actor_position[1] - target_position[1]);
    assert!(
        (20.0..=24.0).contains(&separation),
        "stale target route left gap {separation}"
    );
    assert_eq!(
        world.reference(actor).unwrap().cell,
        Some(soe_esm::GlobalFormId {
            file: 0,
            object: CELL_B
        })
    );
}
