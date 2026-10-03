// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial mutation checks for PRF-11 spatial and sliced queries.

use std::task::Poll;

use soe_navmesh::{NavGraph, NodeId, ObstacleShape};

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
fn prf_11_breaker_spatial_index_rebuilds_after_adding_a_mesh() {
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(
        vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
        &[[0, 1, 2]],
    );
    assert_eq!(graph.locate([2.0, 2.0, 0.0]), Some(NodeId::new(0, 0)));

    graph.add_mesh_with_adjacency(
        vec![[0.0, 0.0, 8.0], [10.0, 0.0, 8.0], [0.0, 10.0, 8.0]],
        &[[0, 1, 2]],
    );
    assert_eq!(graph.locate([2.0, 2.0, 7.0]), Some(NodeId::new(1, 0)));
}

#[test]
fn prf_11_breaker_obstacle_change_rejects_a_pending_corridor() {
    let mut graph = grid(8);
    let from = [0.2, 0.1, 0.0];
    let to = [7.8, 7.9, 0.0];
    let baseline = graph.find_path_between(from, to).expect("initial route");
    let mut search = graph.start_path_between(from, to).expect("search");
    assert!(search.advance(&graph, 1).is_pending());

    graph
        .add_obstacle(ObstacleShape::Cylinder {
            base: [4.0, 4.0, -1.0],
            radius: 20.0,
            height: 3.0,
        })
        .expect("valid obstacle");

    assert_eq!(search.advance(&graph, usize::MAX), Poll::Ready(None));
    assert!(graph.find_path_between(from, to).is_none());
    assert!(!baseline.nodes.is_empty());
}
