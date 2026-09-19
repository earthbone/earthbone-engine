// SPDX-License-Identifier: GPL-3.0-or-later

//! Geometric acceptance checks for Detour-style paths, with fixed tolerances.

use super::{distance, NavGraph, NodeId, Path};

fn tiles(cells: &[(i32, i32)], reverse: bool, height: impl Fn(i32) -> f32) -> NavGraph {
    let mut indices = std::collections::BTreeMap::new();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for &(x, y) in cells {
        let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)].map(|(x, y)| {
            *indices.entry((x, y)).or_insert_with(|| {
                vertices.push([x as f32, y as f32, height(x)]);
                vertices.len() as u32 - 1
            })
        });
        for mut t in [
            [corners[0], corners[1], corners[2]],
            [corners[0], corners[2], corners[3]],
        ] {
            if reverse {
                t.reverse();
            }
            triangles.push(t);
        }
    }
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

fn length(path: &Path) -> f32 {
    path.points.windows(2).map(|p| distance(p[0], p[1])).sum()
}

fn on_corridor(graph: &NavGraph, path: &Path) {
    assert_eq!(path.points.len(), path.point_nodes.len());
    for pair in path.points.windows(2) {
        for step in 0..=100 {
            let t = step as f32 / 100.0;
            let point: [f32; 3] = std::array::from_fn(|i| pair[0][i] * (1.0 - t) + pair[1][i] * t);
            assert!(
                path.nodes
                    .iter()
                    .any(|node| graph.meshes[node.mesh as usize]
                        .height_at(node.triangle, point[0], point[1])
                        .is_some_and(|z| (z - point[2]).abs() < 1e-4)),
                "outside corridor: {point:?}"
            );
        }
    }
}

#[test]
fn flat_strip_reaches_direct_distance_in_both_windings() {
    for reverse in [false, true] {
        let graph = tiles(&[(0, 0), (1, 0), (2, 0), (3, 0)], reverse, |_| 0.0);
        let (from, to) = ([0.1, 0.1, 0.0], [3.9, 0.1, 0.0]);
        let path = graph.find_path_between(from, to).unwrap();
        assert!(
            (length(&path) - 3.8).abs() < 1e-4,
            "pulled length {}",
            length(&path)
        );
        let mut midpoint_route = vec![from];
        for pair in path.nodes.windows(2) {
            let node = pair[0];
            let edge = graph
                .triangle(node)
                .neighbors
                .iter()
                .position(|n| *n == Some(pair[1]))
                .unwrap();
            midpoint_route
                .push(graph.meshes[node.mesh as usize].edge_midpoint(node.triangle, edge));
        }
        midpoint_route.push(to);
        let old_length: f32 = midpoint_route
            .windows(2)
            .map(|p| distance(p[0], p[1]))
            .sum();
        assert!(old_length > length(&path) * 1.01);
        assert_eq!(path.points[0], from);
        assert_eq!(path.points.last(), Some(&to));
        on_corridor(&graph, &path);
    }
}

#[test]
fn bent_corridor_rounds_the_inner_corner_in_both_windings() {
    for reverse in [false, true] {
        let graph = tiles(&[(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)], reverse, |_| 0.0);
        let path = graph
            .find_path_between([0.1, 0.9, 0.0], [2.1, 2.9, 0.0])
            .unwrap();
        on_corridor(&graph, &path);
        let expected = 1.9_f32.hypot(0.1) * 2.0;
        assert!(
            (length(&path) - expected).abs() < 1e-4,
            "length {} expected {expected}",
            length(&path)
        );
    }
}

#[test]
fn portal_intersections_keep_the_ridge_height() {
    let graph = tiles(&[(0, 0), (1, 0), (2, 0)], false, |x| {
        if x == 1 {
            1.0
        } else {
            0.0
        }
    });
    let path = graph
        .find_path_between([0.1, 0.4, 0.1], [2.9, 0.4, 0.0])
        .unwrap();
    assert!(path.points.iter().any(|p| p[2] == 1.0));
    on_corridor(&graph, &path);
}

fn seam(target: Vec<[f32; 3]>) -> NavGraph {
    let mut graph = NavGraph::new();
    let source = graph.add_mesh_with_adjacency(
        vec![[1.0, 0.0, 0.0], [1.0, 2.0, 0.0], [0.0, 1.0, 0.0]],
        &[[0, 1, 2]],
    );
    let destination = graph.add_mesh_with_adjacency(target, &[[0, 1, 2]]);
    graph.link(NodeId::new(source, 0), 0, NodeId::new(destination, 0));
    graph
}

#[test]
fn directed_partial_seam_clips_the_crossing_and_tracks_ownership() {
    let graph = seam(vec![[1.0, 0.5, 0.0], [2.0, 1.0, 0.0], [1.0, 1.5, 0.0]]);
    let (from, to) = ([0.9, 0.1, 0.0], [1.8, 0.9, 0.0]);
    let path = graph.find_path_between(from, to).unwrap();
    assert_eq!(path.points[1], [1.0, 0.5, 0.0]);
    assert_eq!(path.point_nodes[1], NodeId::new(1, 0));
    on_corridor(&graph, &path);
    assert!(graph.find_path_between(to, from).is_none());
}

#[test]
fn arbitrary_gap_remains_an_explicit_directed_transition() {
    let graph = seam(vec![[10.0, 0.0, 0.0], [11.0, 0.0, 0.0], [10.0, 1.0, 0.0]]);
    let path = graph
        .find_path(NodeId::new(0, 0), NodeId::new(1, 0))
        .unwrap();
    assert_eq!(
        path.point_nodes,
        vec![
            NodeId::new(0, 0),
            NodeId::new(0, 0),
            NodeId::new(1, 0),
            NodeId::new(1, 0)
        ]
    );
    assert_eq!(path.points[1], [1.0, 1.0, 0.0]);
    assert!(path.points.iter().flatten().all(|n| n.is_finite()));
    assert!(graph
        .find_path(NodeId::new(1, 0), NodeId::new(0, 0))
        .is_none());
}

#[test]
fn coincident_off_mesh_and_portal_endpoints_are_preserved() {
    let graph = tiles(&[(0, 0), (1, 0)], false, |_| 0.0);
    for (from, to) in [
        ([0.2, 0.1, 0.0], [0.2, 0.1, 0.0]),
        ([1.0, 0.5, 0.0], [1.0, 0.5, 0.0]),
        ([-0.1, 0.5, 0.0], [2.1, 0.5, 0.0]),
    ] {
        let path = graph.find_path_between(from, to).unwrap();
        assert_eq!(path.points.first(), Some(&from));
        assert_eq!(path.points.last(), Some(&to));
        assert!(path.points.iter().flatten().all(|n| n.is_finite()));
    }
    let graph = seam(vec![[1.0, 2.0, 0.0], [2.0, 2.5, 0.0], [1.0, 3.0, 0.0]]);
    let path = graph
        .find_path(NodeId::new(0, 0), NodeId::new(1, 0))
        .unwrap();
    assert!(path.points.contains(&[1.0, 2.0, 0.0]));
}
