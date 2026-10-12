// SPDX-License-Identifier: GPL-3.0-or-later

//! Orbit work comparison against the exhaustive locator at 3cfb4c4e1^.
//! Both runs use the production AI-25 segment and arc sampling predicates.

use soe_navmesh::{NavGraph, NodeId};
use soe_trace::{count, Operation, WorkCounts, WorkFrame};

use super::{orbit_arc_with_locator, orbit_segment_with_locator, WALK_SPEED};

fn dense_square() -> NavGraph {
    let vertices = (0..=64)
        .flat_map(|y| (0..=64).map(move |x| [x as f32 * 6.25, y as f32 * 6.25, 0.0]))
        .collect();
    let triangles: Vec<_> = (0..64)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                let a = y * 65 + x;
                [[a, a + 1, a + 66], [a, a + 66, a + 65]]
            })
        })
        .collect();
    let mut graph = NavGraph::new();
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

fn pre_prf_11_locate(graph: &NavGraph, point: [f32; 3]) -> Option<NodeId> {
    count(Operation::NavmeshLocate, 1);
    if !point.iter().all(|p| p.is_finite()) {
        return None;
    }
    let mut best = None;
    for (mi, mesh) in graph.meshes().iter().enumerate() {
        let (min, max) = mesh.bounds();
        if point[0] < min[0] || point[0] > max[0] || point[1] < min[1] || point[1] > max[1] {
            continue;
        }
        for (ti, triangle) in mesh.triangles.iter().enumerate() {
            count(Operation::NavmeshTriangles, 1);
            let [a, b, c] = triangle.vertices.map(|v| mesh.vertices[v as usize]);
            let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if d.abs() < 1e-9 {
                continue;
            }
            let l0 = ((b[1] - c[1]) * (point[0] - c[0]) + (c[0] - b[0]) * (point[1] - c[1])) / d;
            let l1 = ((c[1] - a[1]) * (point[0] - c[0]) + (a[0] - c[0]) * (point[1] - c[1])) / d;
            let l2 = 1.0 - l0 - l1;
            if l0 < -1e-4 || l1 < -1e-4 || l2 < -1e-4 {
                continue;
            }
            let z = l0 * a[2] + l1 * b[2] + l2 * c[2];
            let candidate = ((z - point[2]).abs(), NodeId::new(mi as u32, ti as u32));
            if !triangle.blocked && best.is_none_or(|old| candidate < old) {
                best = Some(candidate);
            }
        }
    }
    best.map(|(_, node)| node)
}

#[test]
fn prf_14_pre_prf_11_orbit_exceeds_the_same_triangle_budget_by_orders_of_magnitude() {
    let limits = WorkCounts([2_000, 8_000, 256, 0, 0, 0, 0, 0, 2_048, 0]);
    let minimum_legacy_multiple = 100;
    let graph = dense_square();
    assert_eq!(graph.triangle_count(), 8_192);
    let center: [f32; 3] = [200.0, 200.0, 25.0];
    let mut position = [270.0, 200.0, 25.0];
    let mut peak_indexed = 0;
    let mut peak_legacy = 0;
    for tick in 0..3 {
        let angle = (position[1] - center[1]).atan2(position[0] - center[0]);
        let radius = (position[0] - center[0])
            .hypot(position[1] - center[1])
            .clamp(40.0, 80.0);
        let next_angle = angle + (WALK_SPEED * 0.1 / radius.max(1.0)).clamp(0.0, 0.3);
        let destination = [
            center[0] + radius * next_angle.cos(),
            center[1] + radius * next_angle.sin(),
            center[2],
        ];
        let mut queries = Vec::new();
        let indexed = {
            let frame = WorkFrame::begin();
            let mut locate = |point| {
                let node = graph.locate(point);
                queries.push((point, node));
                node.is_some()
            };
            assert!(orbit_segment_with_locator(
                &mut locate,
                position,
                destination
            ));
            assert!(orbit_arc_with_locator(
                &mut locate,
                center,
                radius,
                angle,
                next_angle,
                40.0,
                80.0,
            ));
            frame.counts()
        };
        let legacy = {
            let frame = WorkFrame::begin();
            let mut expected = queries.iter();
            let mut locate = |point| {
                let node = pre_prf_11_locate(&graph, point);
                assert_eq!(expected.next(), Some(&(point, node)));
                node.is_some()
            };
            assert!(orbit_segment_with_locator(
                &mut locate,
                position,
                destination
            ));
            assert!(orbit_arc_with_locator(
                &mut locate,
                center,
                radius,
                angle,
                next_angle,
                40.0,
                80.0,
            ));
            assert!(expected.next().is_none());
            frame.counts()
        };
        assert_eq!(indexed.check(&limits), Ok(()), "tick {tick}: {indexed:?}");
        let triangles = legacy.get(Operation::NavmeshTriangles);
        assert_eq!(
            legacy.check(&limits),
            Err((Operation::NavmeshTriangles, triangles, 8_000)),
            "tick {tick}: {legacy:?}"
        );
        assert_eq!(
            legacy.get(Operation::NavmeshLocate),
            indexed.get(Operation::NavmeshLocate)
        );
        assert_eq!(triangles, queries.len() as u64 * 8_192);
        assert!(triangles >= limits.get(Operation::NavmeshTriangles) * minimum_legacy_multiple);
        assert!(triangles >= indexed.get(Operation::NavmeshTriangles) * minimum_legacy_multiple);
        peak_indexed = peak_indexed.max(indexed.get(Operation::NavmeshTriangles));
        peak_legacy = peak_legacy.max(triangles);
        position = destination;
    }
    println!("PRF_14_ORBIT_TRIANGLES indexed={peak_indexed} pre_prf_11={peak_legacy} limit=8000");
}
