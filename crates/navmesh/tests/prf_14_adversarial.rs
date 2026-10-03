// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial counter checks for invalid queries and disconnected paths.

use soe_navmesh::{NavGraph, NodeId};
use soe_trace::{Operation, WorkFrame};

fn triangle(graph: &mut NavGraph, x: f32) -> NodeId {
    let mesh = graph.add_mesh_with_adjacency(
        vec![[x, 0.0, 0.0], [x + 2.0, 0.0, 0.0], [x, 2.0, 0.0]],
        &[[0, 1, 2]],
    );
    NodeId::new(mesh, 0)
}

#[test]
fn prf_14_rejected_location_attempts_count_calls_and_only_test_candidates() {
    let mut graph = NavGraph::new();
    let node = triangle(&mut graph, 0.0);

    let invalid = WorkFrame::begin();
    assert_eq!(graph.locate([f32::NAN, 0.5, 0.0]), None);
    assert_eq!(invalid.counts().get(Operation::NavmeshLocate), 1);
    assert_eq!(invalid.counts().get(Operation::NavmeshTriangles), 0);
    drop(invalid);

    let outside = WorkFrame::begin();
    assert_eq!(graph.locate([20.0, 20.0, 0.0]), None);
    assert_eq!(outside.counts().get(Operation::NavmeshLocate), 1);
    assert_eq!(outside.counts().get(Operation::NavmeshTriangles), 0);
    drop(outside);

    graph.set_blocked(node, true);
    let blocked = WorkFrame::begin();
    assert_eq!(graph.locate([0.5, 0.5, 0.0]), None);
    assert_eq!(blocked.counts().get(Operation::NavmeshLocate), 1);
    assert_eq!(blocked.counts().get(Operation::NavmeshTriangles), 1);
}

#[test]
fn prf_14_path_counts_include_expansions_but_not_component_short_circuits() {
    let mut graph = NavGraph::new();
    let first = triangle(&mut graph, 0.0);
    let second = triangle(&mut graph, 3.0);
    let third = triangle(&mut graph, 6.0);
    graph.link(first, 1, second);
    graph.link(second, 1, third);

    let connected = WorkFrame::begin();
    assert!(graph.find_path(first, third).is_some());
    assert_eq!(connected.counts().get(Operation::PathNodes), 3);
    drop(connected);

    let disconnected = triangle(&mut graph, 30.0);
    let no_route = WorkFrame::begin();
    assert!(graph.find_path(first, disconnected).is_none());
    assert_eq!(no_route.counts().get(Operation::PathNodes), 0);
}
