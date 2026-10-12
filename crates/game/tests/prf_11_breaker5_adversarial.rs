// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_navmesh::{NavGraph, NodeId};
use std::task::Poll;

fn flat_triangle(z: f32) -> (Vec<[f32; 3]>, [[u32; 3]; 1]) {
    (
        vec![[0.0, 0.0, z], [100.0, 0.0, z], [0.0, 100.0, z]],
        [[0, 1, 2]],
    )
}

#[test]
fn prf_11_breaker5_spatial_index_includes_meshes_added_after_first_query() {
    let mut graph = NavGraph::new();
    let (vertices, triangles) = flat_triangle(0.0);
    graph.add_mesh_with_adjacency(vertices, &triangles);

    assert_eq!(graph.locate([20.0, 20.0, 0.0]), Some(NodeId::new(0, 0)));

    let (vertices, triangles) = flat_triangle(20.0);
    graph.add_mesh_with_adjacency(vertices, &triangles);
    assert_eq!(graph.locate([20.0, 20.0, 19.0]), Some(NodeId::new(1, 0)));
}

#[test]
fn prf_11_breaker5_locate_uses_nearest_unblocked_surface_and_stable_ties() {
    let mut graph = NavGraph::new();
    for z in [0.0, 0.0, 20.0, 40.0] {
        let (vertices, triangles) = flat_triangle(z);
        graph.add_mesh_with_adjacency(vertices, &triangles);
    }

    assert_eq!(graph.locate([20.0, 20.0, 0.0]), Some(NodeId::new(0, 0)));
    graph.set_blocked(NodeId::new(0, 0), true);
    assert_eq!(graph.locate([20.0, 20.0, 0.0]), Some(NodeId::new(1, 0)));
    graph.set_blocked(NodeId::new(2, 0), true);
    assert_eq!(graph.locate([20.0, 20.0, 19.0]), Some(NodeId::new(1, 0)));
}

#[test]
fn prf_11_breaker5_one_node_slices_match_sync_and_consume_result_once() {
    let mut graph = NavGraph::new();
    let (vertices, triangles) = flat_triangle(0.0);
    graph.add_mesh_with_adjacency(vertices, &triangles);
    let from = [7.0, 11.0, 0.0];
    let to = [8.0, 12.0, 0.0];
    let expected = graph
        .find_path_between(from, to)
        .expect("same-triangle path");
    let mut search = graph
        .start_path_between(from, to)
        .expect("same-triangle search");

    assert_eq!(search.advance(&graph, 0), Poll::Pending);
    assert_eq!(search.advance(&graph, 1), Poll::Ready(Some(expected)));
    assert_eq!(search.last_work(), 1);
    assert_eq!(search.advance(&graph, 1), Poll::Ready(None));
    assert_eq!(search.last_work(), 0);
}
