// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-11 bounded triangle selection and sliced search regressions.

use std::task::Poll;

use super::*;

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
fn prf_11_large_mesh_locate_tests_at_most_eight_triangles() {
    let graph = grid(200);
    assert_eq!(graph.triangle_count(), 80_000);
    for point in [[100.2, 110.7, 0.0], [0.0, 0.0, 0.0], [100.0, 100.0, 0.0]] {
        let mut tested = 0;
        assert!(graph.locate_impl(point, |_| tested += 1).is_some());
        assert!(tested <= 8, "tested {tested} of 80,000 triangles");
    }
}

#[test]
fn prf_11_triangle_index_retains_tolerance_floors_blocks_and_ties() {
    let mut graph = NavGraph::new();
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [0.0, 100.0, 0.0],
        [0.0, 0.0, 10.0],
        [100.0, 0.0, 10.0],
        [0.0, 100.0, 10.0],
        [-100.0, -100.0, 0.0],
    ];
    graph.add_mesh_with_adjacency(vertices, &[[0, 1, 2], [3, 4, 5], [0, 1, 2], [0, 1, 6]]);
    assert_eq!(graph.locate([-0.005, 50.0, 0.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([20.0, 20.0, 5.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([20.0, 20.0, 9.0]), Some(NodeId::new(0, 1)));
    graph.set_blocked(NodeId::new(0, 1), true);
    assert_eq!(graph.locate([20.0, 20.0, 9.0]), Some(NodeId::new(0, 0)));
}

#[test]
fn prf_11_overlapping_interiors_prune_distant_surface_heights() {
    let mut graph = NavGraph::new();
    for floor in 0..1024 {
        let z = floor as f32 * 20.0;
        graph.add_mesh_with_adjacency(
            vec![[0.0, 0.0, z], [100.0, 0.0, z], [0.0, 100.0, z]],
            &[[0, 1, 2]],
        );
    }
    for z in [10241.0, 10250.0] {
        let mut tested = 0;
        assert_eq!(
            graph.locate_impl([20.0, 20.0, z], |_| tested += 1),
            Some(NodeId::new(512, 0))
        );
        assert!(tested <= 2, "tested {tested} of 1024 overlapping interiors");
    }
}

#[test]
fn prf_11_slices_obey_budget_and_preserve_every_path_field() {
    let mut graph = grid(20);
    graph.set_flags(NodeId::new(0, 20), WATER_FLAG);
    graph.set_flags(NodeId::new(0, 21), PREFERRED_FLAG);
    let from = [0.2, 0.7, 0.0];
    let to = [19.8, 19.3, 0.0];
    let expected = graph.find_path_between(from, to);
    assert!(expected.is_some());
    for budget in [1, 3, 64] {
        let mut search = graph.start_path_between(from, to).unwrap();
        assert!(search.advance(&graph, 0).is_pending());
        let mut frames = 0;
        loop {
            frames += 1;
            let result = search.advance(&graph, budget);
            assert!(search.last_work() <= budget);
            if let Poll::Ready(path) = result {
                assert_eq!(path, expected);
                break;
            }
            assert!(frames < 2000);
        }
        assert!(frames > 1);
    }
}

#[test]
fn prf_11_graph_changes_cancel_search_and_disconnected_routes_finish() {
    let mut graph = grid(10);
    let mut search = graph
        .start_path_between([0.2, 0.7, 0.0], [9.8, 9.3, 0.0])
        .unwrap();
    assert!(search.advance(&graph, 1).is_pending());
    graph.set_blocked(NodeId::new(0, 0), true);
    assert_eq!(search.advance(&graph, 1), Poll::Ready(None));
    let mut graph = grid(2);
    for node in 0..8 {
        graph.set_blocked(NodeId::new(0, node), true);
    }
    assert!(graph
        .start_path_between([0.2, 0.7, 0.0], [1.8, 1.3, 0.0])
        .is_none());
}

#[test]
fn prf_11_closed_door_index_counts_shared_bindings_and_rebinding() {
    let mut graph = grid(2);
    let a = NodeId::new(0, 0);
    let b = NodeId::new(0, 7);
    for door in [1, 2] {
        graph.add_door_node(door, a);
        graph.add_door_node(door, b);
        graph.bind_door_footprint(door, graph.centroid(a));
        assert!(graph.set_door_open(door, false));
        assert!(!graph.set_door_open(door, false));
    }
    assert!(!graph.is_walkable(a));
    graph.set_door_open(1, true);
    assert!(!graph.is_walkable(a), "the other door still blocks");
    graph.bind_door_footprint(2, graph.centroid(b));
    assert!(graph.is_walkable(a));
    assert!(!graph.is_walkable(b));
    graph.bind_door_footprint(2, [100.0, 100.0, 0.0]);
    assert!(
        graph.is_walkable(b),
        "removing the binding removes its block"
    );
    graph.bind_door_footprint(2, graph.centroid(b));
    assert!(
        !graph.is_walkable(b),
        "a closed door's new binding is blocked"
    );
    graph.set_door_open(2, true);
    assert!(graph.is_walkable(b));
}

#[test]
fn prf_11_nearest_reuses_radius_across_overlapping_meshes() {
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        &[[0, 1, 2]],
    );
    let far = grid(200);
    let mut vertices: Vec<_> = far
        .mesh(0)
        .vertices
        .iter()
        .map(|point| [point[0] + 1000.0, point[1] + 1000.0, point[2]])
        .collect();
    let mut triangles: Vec<_> = far
        .mesh(0)
        .triangles
        .iter()
        .map(|triangle| triangle.vertices)
        .collect();
    let edge = vertices.len() as u32;
    vertices.extend([
        [0.0, 0.0, 0.0],
        [1000.0, 1000.0, 0.0],
        [1001.0, 1000.0, 0.0],
    ]);
    triangles.push([edge, edge + 1, edge + 2]);
    graph.add_mesh_with_adjacency(vertices, &triangles);
    let mut tested = 0;
    assert_eq!(
        graph.nearest_impl([-1.0, -1.0, 0.0], |_| tested += 1),
        Some(NodeId::new(0, 0))
    );
    assert!(tested <= 2, "tested {tested} of 80,002 triangles");
}

#[test]
fn prf_11_containment_stops_after_first_surface_in_overlapping_interiors() {
    let mut graph = NavGraph::new();
    for floor in 0..1024 {
        let z = floor as f32 * 20.0;
        graph.add_mesh_with_adjacency(
            vec![[0.0, 0.0, z], [100.0, 0.0, z], [0.0, 100.0, z]],
            &[[0, 1, 2]],
        );
    }
    graph.set_blocked(NodeId::new(0, 0), true);
    let mut tested = 0;
    assert!(graph.contains_point_impl([20.0, 20.0, 10000.0], |_| tested += 1));
    assert_eq!(
        tested, 1,
        "containment includes blocked triangles and ignores height"
    );
    assert!(!graph.contains_point([200.0, 200.0, 0.0]));
    assert!(!graph.contains_point([f32::NAN, 0.0, 0.0]));
}
