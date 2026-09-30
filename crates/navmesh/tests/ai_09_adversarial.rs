// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::navm::{DoorLink, Geometry, NavGrid, Parent, Triangle};
use soe_navmesh::vanilla::Builder;
use soe_navmesh::NodeId;

fn geometry() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(0x100),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
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
            door: 0x200,
        }],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [1.0, 1.0],
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

#[test]
fn ai_09_repeated_close_and_reopen_changes_only_the_door_triangle() {
    let mut builder = Builder::new();
    builder.add(0x100, &geometry(), |form| Some(u64::from(form)));
    let (mut graph, _) = builder.build();
    let door_node = graph.door_nodes(0x200)[0];
    let other = NodeId::new(0, 1);

    // A runtime door binding should close the NAVM-named triangle and make
    // reopening idempotently restore that same triangle.
    graph.set_blocked(door_node, true);
    graph.set_blocked(door_node, true);
    assert!(graph.triangle(door_node).blocked);
    assert!(!graph.triangle(other).blocked);
    graph.set_blocked(door_node, false);
    graph.set_blocked(door_node, false);
    assert!(!graph.triangle(door_node).blocked);
    assert!(!graph.triangle(other).blocked);
}

#[test]
fn ai_09_closed_door_triangle_stops_path_and_reopening_restores_path() {
    let mut builder = Builder::new();
    builder.add(0x100, &geometry(), |form| Some(u64::from(form)));
    let (mut graph, _) = builder.build();
    let door_node = graph.door_nodes(0x200)[0];
    let other = NodeId::new(0, 1);

    graph.set_blocked(door_node, true);
    assert!(graph.find_path(door_node, other).is_none());
    graph.set_blocked(door_node, false);
    assert!(graph.find_path(door_node, other).is_some());
}
