// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! TST-18 regression checks for bounded NAVM lookup work.

use soe_navmesh::NavGraph;
use soe_trace::{Operation, WorkFrame};

fn grid(size: u32) -> NavGraph {
    let mut graph = NavGraph::new();
    let vertices = (0..=size)
        .flat_map(|y| (0..=size).map(move |x| [x as f32, y as f32, 0.0]))
        .collect();
    let mut triangles = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let a = y * (size + 1) + x;
            triangles.push([a, a + 1, a + size + 2]);
            triangles.push([a, a + size + 2, a + size + 1]);
        }
    }
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

#[test]
fn tst_18_large_navmesh_locate_stays_within_tick_triangle_budget() {
    let graph = grid(100);
    assert_eq!(graph.triangle_count(), 20_000);

    let frame = WorkFrame::begin();
    assert!(graph.locate([50.2, 55.7, 0.0]).is_some());
    let examined = frame.counts().get(Operation::NavmeshTriangles);
    assert!(examined <= 4_000, "lookup examined {examined} triangles");
}
