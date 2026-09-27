// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_navmesh::{NavGraph, NodeId, ObstacleShape};

fn grid() -> NavGraph {
    let vertices = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]];
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &[[0, 1, 2], [0, 2, 3]]);
    graph
}

#[test]
fn ai_17_obstacle_height_and_later_removal_recompute_routes() {
    let mut graph = grid();
    let start = NodeId::new(0, 0);
    let end = NodeId::new(0, 1);
    assert!(graph.find_path(start, end).is_some());

    let below_mesh = graph.add_obstacle(ObstacleShape::Box {
        min: [10.0, 10.0, -20.0],
        max: [90.0, 90.0, -10.0],
    }).unwrap();
    assert!(graph.find_path(start, end).is_some(), "a vertically separate obstacle leaves the route open");
    assert!(graph.remove_obstacle(below_mesh));

    let blocking = graph.add_obstacle(ObstacleShape::Box {
        min: [10.0, 10.0, -1.0],
        max: [90.0, 90.0, 1.0],
    }).unwrap();
    assert!(graph.find_path(start, end).is_none(), "both intersecting triangles are blocked");
    assert!(graph.remove_obstacle(blocking));
    assert!(graph.find_path(start, end).is_some(), "removal restores connectivity");
}

#[test]
fn ai_17_connectivity_queries_exclude_obstacle_blocked_triangles() {
    let mut graph = grid();
    let blocked = NodeId::new(0, 0);
    let obstacle = graph.add_obstacle(ObstacleShape::Box {
        min: [70.0, 10.0, -1.0],
        max: [90.0, 20.0, 1.0],
    }).unwrap();
    let reachable = graph.reachable(NodeId::new(0, 1));
    assert!(!reachable.contains(&blocked), "blocked NAVM polygons are not reachable");
    assert!(graph.remove_obstacle(obstacle));
}
