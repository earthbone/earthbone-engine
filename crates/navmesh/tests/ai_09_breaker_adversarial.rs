// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::navm::{DoorLink, Geometry, NavGrid, Parent, Triangle as NavTriangle};
use soe_navmesh::vanilla::Builder;
use soe_navmesh::NodeId;

fn square(z: f32, door_links: Vec<DoorLink>) -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(0x100),
        vertices: vec![
            [0.0, 0.0, z],
            [10.0, 0.0, z],
            [10.0, 10.0, z],
            [0.0, 10.0, z],
        ],
        triangles: vec![
            NavTriangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            NavTriangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links,
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [10.0, 10.0],
            min: [0.0, 0.0, z],
            max: [10.0, 10.0, z],
            cells: vec![vec![0, 1]],
        },
    }
}

fn door_link(triangle: i16, door: u32) -> DoorLink {
    DoorLink {
        triangle,
        crc: 0,
        door,
    }
}

#[test]
fn ai_09_door_footprint_does_not_close_overlapping_navmesh_on_another_floor() {
    let mut builder = Builder::new();
    builder.add(0x1000, &square(0.0, vec![door_link(0, 0x200)]), |form| {
        Some(u64::from(form))
    });
    builder.add(0x1001, &square(100.0, vec![door_link(0, 0x200)]), |form| {
        Some(u64::from(form))
    });
    let (mut graph, _) = builder.build();

    assert_eq!(graph.door_nodes(0x200).len(), 2);
    assert!(graph.bind_door_footprint(0x200, [7.0, 2.0, 0.0]));
    assert!(graph.set_door_open(0x200, false));

    assert!(graph
        .find_path(NodeId::new(1, 0), NodeId::new(1, 1))
        .is_some());
    assert!(graph
        .find_path(NodeId::new(0, 0), NodeId::new(0, 1))
        .is_none());
}

#[test]
fn ai_09_closing_one_door_keeps_unlinked_triangle_paths_available() {
    let mut builder = Builder::new();
    builder.add(0x1000, &square(0.0, vec![door_link(0, 0x200)]), |form| {
        Some(u64::from(form))
    });
    let (mut graph, _) = builder.build();
    let unrelated = NodeId::new(0, 1);

    assert!(graph.bind_door_footprint(0x200, [7.0, 2.0, 0.0]));
    assert!(graph.set_door_open(0x200, false));
    assert!(graph.find_path(unrelated, unrelated).is_some());
    assert!(graph.find_path(NodeId::new(0, 0), unrelated).is_none());
}

#[test]
fn ai_09_repeated_state_updates_preserve_the_linked_triangle_state() {
    let mut builder = Builder::new();
    builder.add(0x1000, &square(0.0, vec![door_link(0, 0x200)]), |form| {
        Some(u64::from(form))
    });
    let (mut graph, _) = builder.build();
    let linked = NodeId::new(0, 0);
    let other = NodeId::new(0, 1);

    assert!(graph.bind_door_footprint(0x200, [7.0, 2.0, 0.0]));
    assert!(graph.set_door_open(0x200, false));
    assert!(!graph.set_door_open(0x200, false));
    assert!(graph.find_path(linked, other).is_none());
    assert!(graph.set_door_open(0x200, true));
    assert!(!graph.set_door_open(0x200, true));
    assert!(graph.find_path(linked, other).is_some());
}
