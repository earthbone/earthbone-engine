// SPDX-License-Identifier: GPL-3.0-or-later

//! Query oracles: exhaustive point selection and a small Dijkstra search
//! independent of the production heap and its component cache.

use proptest::prelude::*;

use super::{distance, spatial::SpatialIndex, NavGraph, NodeId, PREFERRED_FLAG, WATER_FLAG};

fn oracle_traversal_cost(flags: u16, water: f32, preferred: f32) -> f32 {
    let mut multiplier = 1.0;
    if flags & 0x0200 != 0 {
        multiplier *= water;
    }
    if flags & 0x0040 != 0 {
        multiplier *= preferred;
    }
    multiplier
}

fn add_triangle(graph: &mut NavGraph, x: f32, y: f32, z: f32) -> u32 {
    graph.add_mesh_with_adjacency(
        vec![[x, y, z], [x + 4.0, y, z], [x, y + 4.0, z]],
        &[[0, 1, 2]],
    )
}

fn exhaustive(graph: &NavGraph, point: [f32; 3], locate: bool) -> Option<NodeId> {
    let mut candidates = Vec::new();
    for (mi, mesh) in graph.meshes.iter().enumerate() {
        for ti in 0..mesh.triangles.len() {
            let d = if locate {
                if point[0] < mesh.min[0]
                    || point[0] > mesh.max[0]
                    || point[1] < mesh.min[1]
                    || point[1] > mesh.max[1]
                {
                    continue;
                }
                let Some(z) = mesh.height_at(ti as u32, point[0], point[1]) else {
                    continue;
                };
                (z - point[2]).abs()
            } else {
                distance(mesh.centroid(ti as u32), point)
            };
            candidates.push((d, NodeId::new(mi as u32, ti as u32)));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    candidates.first().map(|(_, id)| *id)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 192, ..ProptestConfig::default() })]

    #[test]
    fn indexed_queries_match_exhaustive_selection(
        meshes in prop::collection::vec((-50i16..50, -50i16..50, -20i16..20), 0..80),
        query in (-55i16..55, -55i16..55, -25i16..25),
    ) {
        let mut graph = NavGraph::new();
        for (x, y, z) in meshes {
            add_triangle(&mut graph, f32::from(x), f32::from(y), f32::from(z));
        }
        let point = [f32::from(query.0), f32::from(query.1), f32::from(query.2)];
        prop_assert_eq!(graph.locate(point), exhaustive(&graph, point, true));
        prop_assert_eq!(graph.nearest(point), exhaustive(&graph, point, false));
        add_triangle(&mut graph, point[0] - 1.0, point[1] - 1.0, point[2]);
        prop_assert_eq!(graph.locate(point), exhaustive(&graph, point, true));
        prop_assert_eq!(graph.nearest(point), exhaustive(&graph, point, false));
    }

    #[test]
    fn nearest_matches_exhaustive_for_large_finite_coordinates(
        vertices in prop::collection::vec(prop::array::uniform3(prop::array::uniform3(-1.0e18f32..1.0e18)), 1..24),
        point in prop::array::uniform3(-1.0e18f32..1.0e18),
    ) {
        let mut graph = NavGraph::new();
        for triangle in vertices {
            graph.add_mesh_with_adjacency(triangle.to_vec(), &[[0, 1, 2]]);
        }
        prop_assert_eq!(graph.nearest(point), exhaustive(&graph, point, false));
    }

    #[test]
    fn a_star_agrees_with_dijkstra_on_directed_blocked_graphs(
        nodes in prop::collection::vec((any::<bool>(), prop::sample::select(vec![0, PREFERRED_FLAG, WATER_FLAG, PREFERRED_FLAG | WATER_FLAG]), prop::array::uniform3(0usize..24), -50i16..50, -50i16..50), 2..24),
        from in any::<usize>(), to in any::<usize>(),
        water in 0.1f32..10.0,
        preferred in 0.1f32..10.0,
    ) {
        let mut graph = NavGraph::new();
        let n = nodes.len();
        for (_, _, _, x, y) in &nodes {
            add_triangle(&mut graph, f32::from(*x), f32::from(*y), 0.0);
        }
        for (i, (blocked, flags, next, _, _)) in nodes.iter().enumerate() {
            let id = NodeId::new(i as u32, 0);
            graph.set_blocked(id, *blocked);
            prop_assert!(graph.set_flags(id, *flags));
            for (edge, neighbor) in next.iter().enumerate() {
                if *neighbor < n {
                    graph.link(id, edge, NodeId::new(*neighbor as u32, 0));
                }
            }
        }
        let (from, to) = (from % n, to % n);
        let mut costs = vec![f32::INFINITY; n];
        let mut visited = vec![false; n];
        if !nodes[from].0 { costs[from] = 0.0; }
        for _ in 0..n {
            let current = (0..n).filter(|i| !visited[*i]).min_by(|a, b| costs[*a].total_cmp(&costs[*b]));
            let Some(current) = current else { break };
            visited[current] = true;
            for &next in &nodes[current].2 {
                if next >= n || nodes[next].0 { continue; }
                let edge = distance(graph.centroid(NodeId::new(current as u32, 0)), graph.centroid(NodeId::new(next as u32, 0))) * oracle_traversal_cost(nodes[next].1, water, preferred);
                costs[next] = costs[next].min(costs[current] + edge);
            }
        }
        let path = graph.find_path_with_costs(
            NodeId::new(from as u32, 0),
            NodeId::new(to as u32, 0),
            super::PathCosts { water, preferred },
        );
        prop_assert_eq!(path.is_some(), costs[to].is_finite());
        if let Some(path) = path {
            prop_assert!((path.cost - costs[to]).abs() <= 0.001, "A* {}, oracle {}", path.cost, costs[to]);
            for step in path.nodes.windows(2) {
                prop_assert!(graph.triangle(step[0]).neighbors.contains(&Some(step[1])));
                prop_assert!(!graph.triangle(step[1]).blocked);
            }
        }
    }
}

#[test]
fn separated_world_queries_prune_at_least_ninety_percent_of_meshes() {
    let mut graph = NavGraph::new();
    for i in 0..1024 {
        add_triangle(&mut graph, i as f32 * 100.0, 0.0, 0.0);
    }
    let index = SpatialIndex::build(&graph.meshes);
    let point = [51201.0, 1.0, 0.0];
    let mut located = 0;
    index.visit_xy(point, |_| located += 1);
    let mut nearest = 0;
    index.visit_nearest(point, |mi, radius| {
        nearest += 1;
        *radius = radius.min(distance(graph.meshes[mi].centroid(0), point));
    });
    assert!(
        located < 103 && nearest < 103,
        "visited {located} and {nearest} of 1024 meshes"
    );
}

#[test]
fn ties_keep_the_first_node_and_links_invalidate_components() {
    let mut graph = NavGraph::new();
    let a = add_triangle(&mut graph, 0.0, 0.0, 0.0);
    let b = add_triangle(&mut graph, 0.0, 0.0, 0.0);
    let (a, b) = (NodeId::new(a, 0), NodeId::new(b, 0));
    assert_eq!(graph.nearest([1.0, 1.0, 0.0]), Some(a));
    assert_eq!(graph.locate([1.0, 1.0, 0.0]), Some(a));
    assert!(graph.find_path(a, b).is_none());
    graph.link(a, 0, b);
    assert!(graph.find_path(a, b).is_some());
    assert!(
        graph.find_path(b, a).is_none(),
        "a directed link stays directed"
    );
    graph.set_blocked(b, true);
    assert!(graph.find_path(a, b).is_none());
    graph.set_blocked(b, false);
    assert!(graph.find_path(a, b).is_some());
}

#[test]
fn invalid_query_nodes_and_nonfinite_points_return_none() {
    let mut graph = NavGraph::new();
    add_triangle(&mut graph, 0.0, 0.0, 0.0);
    assert!(graph
        .find_path(NodeId::new(99, 0), NodeId::new(0, 0))
        .is_none());
    assert!(graph
        .find_path(NodeId::new(0, 0), NodeId::new(0, 99))
        .is_none());
    assert!(graph.locate([f32::NAN, 0.0, 0.0]).is_none());
    assert!(graph.nearest([f32::INFINITY, 0.0, 0.0]).is_none());
    assert!(!graph.set_flags(NodeId::new(99, 0), WATER_FLAG));
    assert!(!graph.set_flags(NodeId::new(0, 99), WATER_FLAG));
}

#[test]
fn invalid_path_costs_return_none() {
    let mut graph = NavGraph::new();
    let a = NodeId::new(add_triangle(&mut graph, 0.0, 0.0, 0.0), 0);
    let b = NodeId::new(add_triangle(&mut graph, 4.0, 0.0, 0.0), 0);
    graph.link(a, 0, b);
    for costs in [
        super::PathCosts {
            water: 0.0,
            preferred: 1.0,
        },
        super::PathCosts {
            water: -1.0,
            preferred: 1.0,
        },
        super::PathCosts {
            water: f32::NAN,
            preferred: 1.0,
        },
        super::PathCosts {
            water: f32::INFINITY,
            preferred: 1.0,
        },
        super::PathCosts {
            water: f32::MAX,
            preferred: 2.0,
        },
        super::PathCosts {
            water: f32::MIN_POSITIVE,
            preferred: f32::MIN_POSITIVE,
        },
    ] {
        assert!(graph.find_path_with_costs(a, b, costs).is_none());
    }
}
