// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial invalidation checks for retained NAVM searches.

use std::task::Poll;

use soe_navmesh::{NavGraph, NodeId};
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
fn prf_11_breaker3_flag_change_cancels_pending_query_and_retry_matches_sync() {
    let mut graph = grid(24);
    let from = [0.2, 0.7, 0.0];
    let to = [23.8, 23.3, 0.0];
    let mut search = graph.start_path_between(from, to).expect("initial route");
    assert_eq!(search.advance(&graph, 1), Poll::Pending);

    assert!(graph.set_flags(NodeId::new(0, 100), 0x0200));
    assert_eq!(search.advance(&graph, 64), Poll::Ready(None));

    let expected = graph
        .find_path_between(from, to)
        .expect("route after flag change");
    let mut retry = graph.start_path_between(from, to).expect("retry starts");
    let mut result = Poll::Pending;
    for _ in 0..10_000 {
        if result.is_ready() {
            break;
        }
        result = retry.advance(&graph, 3);
        assert!(retry.last_work() <= 3);
    }
    assert_eq!(result, Poll::Ready(Some(expected)));
}

#[test]
fn prf_11_breaker3_adding_unrelated_mesh_cancels_search_without_changing_route() {
    let mut graph = grid(18);
    let from = [0.2, 0.7, 0.0];
    let to = [17.8, 17.3, 0.0];
    let expected = graph.find_path_between(from, to).expect("initial route");
    let mut search = graph.start_path_between(from, to).expect("search starts");
    assert_eq!(search.advance(&graph, 1), Poll::Pending);

    graph.add_mesh_with_adjacency(
        vec![
            [1000.0, 1000.0, 0.0],
            [1001.0, 1000.0, 0.0],
            [1000.0, 1001.0, 0.0],
        ],
        &[[0, 1, 2]],
    );
    assert_eq!(search.advance(&graph, 64), Poll::Ready(None));
    assert_eq!(graph.find_path_between(from, to), Some(expected));
}

#[test]
fn prf_11_breaker3_non_finite_endpoints_are_rejected_for_sliced_and_sync_queries() {
    let graph = grid(4);
    for (from, to) in [
        ([f32::NAN, 0.5, 0.0], [3.5, 3.0, 0.0]),
        ([0.5, 0.0, 0.0], [3.5, f32::INFINITY, 0.0]),
    ] {
        assert!(graph.start_path_between(from, to).is_none());
        assert!(graph.find_path_between(from, to).is_none());
    }
}

#[test]
fn prf_11_breaker3_large_mesh_locate_tests_at_most_eight_triangles() {
    let graph = grid(200);
    assert_eq!(graph.triangle_count(), 80_000);
    let frame = WorkFrame::begin();
    assert!(graph.locate([100.2, 110.7, 0.0]).is_some());
    assert!(frame.counts().get(Operation::NavmeshTriangles) <= 8);
}
