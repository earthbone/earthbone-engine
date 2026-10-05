// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for PRF-11 indexed location and sliced routes.

use std::task::Poll;

use soe_navmesh::{NavGraph, NodeId};

#[test]
fn prf_11_breaker2_locate_chooses_nearest_unblocked_overlapping_surface() {
    let floors = 512u32;
    let mut vertices = Vec::with_capacity((floors * 3) as usize);
    let mut triangles = Vec::with_capacity(floors as usize);
    for floor in 0..floors {
        let z = floor as f32 * 2.0;
        let first = vertices.len() as u32;
        vertices.extend([[0.0, 0.0, z], [100.0, 0.0, z], [0.0, 100.0, z]]);
        triangles.push([first, first + 1, first + 2]);
    }
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);

    assert_eq!(graph.locate([20.0, 20.0, 600.0]), Some(NodeId::new(0, 300)));
    graph.set_blocked(NodeId::new(0, 300), true);
    assert_eq!(
        graph.locate([20.0, 20.0, 600.0]),
        Some(NodeId::new(0, 299)),
        "equidistant surfaces select the lower node id"
    );
    assert_eq!(graph.locate([200.0, 200.0, 600.0]), None);
}

#[test]
fn prf_11_breaker2_sliced_large_route_matches_synchronous_after_graph_change() {
    let size = 48u32;
    let mut vertices = Vec::with_capacity(((size + 1) * (size + 1)) as usize);
    for y in 0..=size {
        for x in 0..=size {
            vertices.push([x as f32, y as f32, 0.0]);
        }
    }
    let mut triangles = Vec::with_capacity((size * size * 2) as usize);
    for y in 0..size {
        for x in 0..size {
            let a = y * (size + 1) + x;
            triangles.push([a, a + 1, a + size + 2]);
            triangles.push([a, a + size + 2, a + size + 1]);
        }
    }
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);
    let from = [0.2, 0.7, 0.0];
    let to = [47.8, 47.3, 0.0];
    let expected = graph.find_path_between(from, to).expect("route exists");

    for budget in [1, 7, 64] {
        let mut search = graph.start_path_between(from, to).expect("search starts");
        assert_eq!(search.advance(&graph, 0), Poll::Pending);
        let mut completed = None;
        for _ in 0..20_000 {
            match search.advance(&graph, budget) {
                Poll::Pending => assert!(search.last_work() <= budget),
                Poll::Ready(path) => {
                    completed = path;
                    break;
                }
            }
        }
        assert_eq!(completed, Some(expected.clone()));
    }

    let mut interrupted = graph
        .start_path_between(from, to)
        .expect("search starts before invalidation");
    assert_eq!(interrupted.advance(&graph, 1), Poll::Pending);
    graph.set_blocked(NodeId::new(0, 0), true);
    assert_eq!(interrupted.advance(&graph, 1), Poll::Ready(None));
}
