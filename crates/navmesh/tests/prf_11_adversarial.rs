// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for PRF-11 point lookup and sliced path queries.

use std::task::Poll;

use soe_navmesh::{NavGraph, NodeId};

fn triangle(z: f32) -> (Vec<[f32; 3]>, [[u32; 3]; 1]) {
    (
        vec![[0.0, 0.0, z], [100.0, 0.0, z], [0.0, 100.0, z]],
        [[0, 1, 2]],
    )
}

#[test]
fn prf_11_adversarial_locate_handles_non_finite_points_and_exact_floor_ties() {
    let mut graph = NavGraph::new();
    for z in [0.0, 20.0] {
        let (vertices, triangles) = triangle(z);
        graph.add_mesh_with_adjacency(vertices, &triangles);
    }

    assert_eq!(graph.locate([20.0, 20.0, 10.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([20.0, 20.0, 20.0]), Some(NodeId::new(1, 0)));
    assert_eq!(graph.locate([f32::NAN, 20.0, 0.0]), None);
    assert_eq!(graph.locate([20.0, f32::INFINITY, 0.0]), None);
}

#[test]
fn prf_11_adversarial_sliced_path_retains_off_mesh_endpoints() {
    let mut graph = NavGraph::new();
    let (vertices, triangles) = triangle(0.0);
    graph.add_mesh_with_adjacency(vertices, &triangles);
    let from = [-4.0, 20.0, 3.0];
    let to = [20.0, 20.0, 7.0];
    let expected = graph.find_path_between(from, to).unwrap();
    let mut search = graph.start_path_between(from, to).unwrap();

    assert!(search.advance(&graph, 0).is_pending());
    assert_eq!(
        search.advance(&graph, 1),
        Poll::Ready(Some(expected.clone()))
    );
    assert_eq!(expected.points.first(), Some(&from));
    assert_eq!(expected.points.last(), Some(&to));
}

#[test]
fn prf_11_adversarial_block_change_cancels_pending_search() {
    let mut graph = NavGraph::new();
    let size = 12u32;
    let vertices = (0..=size)
        .flat_map(|y| (0..=size).map(move |x| [x as f32 * 10.0, y as f32 * 10.0, 0.0]))
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
    let start = [1.0, 1.0, 0.0];
    let end = [119.0, 119.0, 0.0];
    let mut search = graph.start_path_between(start, end).unwrap();
    assert!(search.advance(&graph, 1).is_pending());

    let blocked_triangle = graph.locate([60.0, 60.0, 0.0]).unwrap();
    graph.set_blocked(blocked_triangle, true);
    assert_eq!(search.advance(&graph, 64), Poll::Ready(None));
}
