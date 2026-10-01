// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runtime obstacle queries preserve NAVM adjacency while changing routes.

use soe_navmesh::{NavGraph, NodeId, ObstacleShape};

fn grid(x: f32) -> NavGraph {
    let mut vertices = Vec::new();
    for y in 0..=5 {
        for column in 0..=5 {
            vertices.push([x + column as f32 * 100.0, y as f32 * 100.0, 0.0]);
        }
    }
    let mut triangles = Vec::new();
    for y in 0..5u32 {
        for column in 0..5u32 {
            let lower_left = y * 6 + column;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + 6;
            let upper_right = upper_left + 1;
            triangles.push([lower_left, lower_right, upper_right]);
            triangles.push([lower_left, upper_right, upper_left]);
        }
    }
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

fn adjacency(graph: &NavGraph) -> Vec<Vec<[Option<NodeId>; 3]>> {
    graph
        .meshes()
        .iter()
        .map(|mesh| {
            mesh.triangles
                .iter()
                .map(|triangle| triangle.neighbors)
                .collect()
        })
        .collect()
}

#[test]
fn ai_17_adding_obstacle_routes_around_affected_triangles_without_topology_changes() {
    let mut graph = grid(0.0);
    let before = graph
        .find_path(NodeId::new(0, 0), NodeId::new(0, 49))
        .unwrap();
    let affected = before.nodes[before.nodes.len() / 2];
    let center = graph.centroid(affected);
    let topology = adjacency(&graph);

    let obstacle = graph
        .add_obstacle(ObstacleShape::Cylinder {
            base: center,
            radius: 1.0,
            height: 1.0,
        })
        .unwrap();
    let after = graph
        .find_path(NodeId::new(0, 0), NodeId::new(0, 49))
        .unwrap();

    assert_ne!(after.nodes, before.nodes, "the affected route changed");
    assert!(
        !after.nodes.contains(&affected),
        "the route avoids the obstacle"
    );
    assert_eq!(adjacency(&graph), topology, "NAVM links remain unchanged");
    assert!(
        graph.remove_obstacle(obstacle),
        "the obstacle was registered"
    );
}

#[test]
fn ai_17_removing_obstacle_restores_the_original_route() {
    let mut graph = grid(0.0);
    let from = NodeId::new(0, 0);
    let to = NodeId::new(0, 49);
    let before = graph.find_path(from, to).unwrap();
    let center = graph.centroid(before.nodes[before.nodes.len() / 2]);
    let obstacle = graph
        .add_obstacle(ObstacleShape::Box {
            min: [center[0] - 1.0, center[1] - 1.0, -1.0],
            max: [center[0] + 1.0, center[1] + 1.0, 1.0],
        })
        .unwrap();
    assert_ne!(graph.find_path(from, to).unwrap().nodes, before.nodes);

    assert!(graph.remove_obstacle(obstacle), "the obstacle was removed");
    let after = graph.find_path(from, to).unwrap();
    assert_eq!(after.nodes, before.nodes, "the original route was restored");
}

#[test]
fn ai_17_unrelated_route_and_topology_stay_unchanged() {
    let mut graph = grid(0.0);
    let far = grid(1000.0);
    for mesh in far.meshes() {
        graph.add_mesh_with_adjacency(
            mesh.vertices.clone(),
            &mesh
                .triangles
                .iter()
                .map(|triangle| triangle.vertices)
                .collect::<Vec<_>>(),
        );
    }
    let from = NodeId::new(1, 0);
    let to = NodeId::new(1, 49);
    let before = graph.find_path(from, to).unwrap();
    let original_topology = adjacency(&graph);
    let affected = graph
        .find_path(NodeId::new(0, 0), NodeId::new(0, 49))
        .unwrap();
    let center = graph.centroid(affected.nodes[affected.nodes.len() / 2]);
    let obstacle = graph
        .add_obstacle(ObstacleShape::Cylinder {
            base: center,
            radius: 1.0,
            height: 1.0,
        })
        .unwrap();

    let after = graph.find_path(from, to).unwrap();
    assert_eq!(
        after.nodes, before.nodes,
        "the unrelated route is unchanged"
    );
    assert_eq!(
        adjacency(&graph),
        original_topology,
        "all links are unchanged"
    );
    assert!(graph.remove_obstacle(obstacle), "the obstacle was removed");
}

#[test]
fn ai_17_invalid_obstacle_geometry_is_rejected() {
    let mut graph = grid(0.0);
    assert!(
        graph
            .add_obstacle(ObstacleShape::Cylinder {
                base: [0.0, 0.0, 0.0],
                radius: 0.0,
                height: 1.0,
            })
            .is_none(),
        "zero radius is invalid"
    );
}

#[test]
fn ai_17_cylinder_obstacle_uses_finite_positive_dimensions() {
    let mut graph = grid(0.0);
    let center = graph.centroid(NodeId::new(0, 2));
    let obstacle = graph
        .add_obstacle(ObstacleShape::Cylinder {
            base: center,
            radius: 20.0,
            height: 128.0,
        })
        .unwrap();
    assert!(
        graph.remove_obstacle(obstacle),
        "the cylinder was registered"
    );
}

#[test]
fn ai_17_obstacle_blocks_triangle_when_footprint_intersects_away_from_centroid() {
    let mut graph = grid(0.0);
    let node = NodeId::new(0, 0);
    assert!(graph.find_path(node, node).is_some());
    let obstacle = graph
        .add_obstacle(ObstacleShape::Box {
            min: [10.0, 10.0, -1.0],
            max: [15.0, 15.0, 1.0],
        })
        .unwrap();

    assert!(
        graph.find_path(node, node).is_none(),
        "intersection with the triangle footprint blocks it"
    );
    assert!(graph.remove_obstacle(obstacle));
}
