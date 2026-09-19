// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every NAVM under Tamriel, stitched into one graph. Runs only when
//! `SKYRIM_DATA` points at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" \
//!   cargo test -p soe-navmesh --release --test real_files -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::records::navm::NavMesh;
use soe_esm::{tag, LoadOrder, Plugin};
use soe_navmesh::vanilla;
use soe_navmesh::NodeId;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

#[test]
fn tamriel_stitches_into_one_graph_and_paths_cross_cells() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order.add(name, plugin).unwrap();
    }
    let tamriel = order
        .winners_of(tag::WRLD)
        .find(|(_, r)| r.editor_id().as_deref() == Some("Tamriel"))
        .map(|(id, _)| id)
        .expect("Tamriel exists");
    let tamriel_raw = order.record(order.overrides(tamriel)[0]).form_id;

    let started = std::time::Instant::now();
    let (graph, stats) =
        vanilla::from_load_order(&order, |r| r.placement.world == Some(tamriel_raw));
    eprintln!(
        "graph built in {:.2}s: {stats:?}",
        started.elapsed().as_secs_f32()
    );
    assert!(stats.meshes >= 13_000, "{} meshes", stats.meshes);
    assert_eq!(
        stats.edge_links,
        stats.resolved_links + stats.unresolved_links + stats.repaired_links
    );
    eprintln!(
        "deleted navmeshes under Tamriel: {}, links to them cut: {}",
        stats.deleted_meshes, stats.repaired_links
    );
    assert!(
        stats.resolved_links * 10 > stats.edge_links * 9,
        "fewer than 90 percent of edge links resolved: {stats:?}"
    );

    // The largest navmesh: a path from its first to its last triangle, or
    // between the first pair that is connected.
    let (largest, _) = graph
        .meshes()
        .iter()
        .enumerate()
        .max_by_key(|(_, m)| m.triangles.len())
        .unwrap();
    let count = graph.mesh(largest as u32).triangles.len() as u32;
    let mut found = None;
    'outer: for a in 0..count.min(20) {
        for b in (count.saturating_sub(20)..count).rev() {
            if let Some(p) = graph.find_path(
                NodeId::new(largest as u32, a),
                NodeId::new(largest as u32, b),
            ) {
                found = Some(p);
                break 'outer;
            }
        }
    }
    let path = found.expect("the largest navmesh has a path across it");
    eprintln!(
        "largest navmesh: {count} triangles, path of {} triangles, cost {:.0}",
        path.nodes.len(),
        path.cost
    );

    // Across an edge link: reachability from one triangle must leave its
    // mesh, and a path to a linked neighbour must exist.
    let mut crossed = 0;
    let mut checked = 0;
    for (mi, mesh) in graph.meshes().iter().enumerate().take(400) {
        for (ti, t) in mesh.triangles.iter().enumerate() {
            let Some(other) = t.neighbors.iter().flatten().find(|n| n.mesh != mi as u32) else {
                continue;
            };
            checked += 1;
            if graph
                .find_path(NodeId::new(mi as u32, ti as u32), *other)
                .is_some()
            {
                crossed += 1;
            }
            break;
        }
        if checked >= 50 {
            break;
        }
    }
    eprintln!("cross-mesh paths: {crossed} of {checked}");
    assert!(checked > 0, "no cross-mesh link found in the first meshes");
    assert_eq!(crossed, checked);

    let mut sampled = 0;
    let mut shortened = 0;
    for (mi, mesh) in graph.meshes().iter().enumerate() {
        if mesh.triangles.len() < 4 {
            continue;
        }
        let Some(path) = graph.find_path(
            NodeId::new(mi as u32, 0),
            NodeId::new(mi as u32, mesh.triangles.len() as u32 - 1),
        ) else {
            continue;
        };
        if path.points.len() != path.nodes.len() + 1 {
            continue;
        }
        let mut old = vec![path.points[0]];
        for pair in path.nodes.windows(2) {
            let mesh = graph.mesh(pair[0].mesh);
            let triangle = &mesh.triangles[pair[0].triangle as usize];
            let edge = triangle
                .neighbors
                .iter()
                .position(|n| *n == Some(pair[1]))
                .unwrap();
            old.push(mesh.edge_midpoint(pair[0].triangle, edge));
        }
        old.push(*path.points.last().unwrap());
        let xy_length = |points: &[[f32; 3]]| -> f64 {
            points
                .windows(2)
                .map(|p| {
                    (f64::from(p[0][0]) - f64::from(p[1][0]))
                        .hypot(f64::from(p[0][1]) - f64::from(p[1][1]))
                })
                .sum()
        };
        let (before, after) = (xy_length(&old), xy_length(&path.points));
        assert!(
            after <= before + 0.25,
            "route {mi} lengthened from {before} to {after}"
        );
        assert_eq!(path.points.len(), path.point_nodes.len());
        for (pair, node) in path.points.windows(2).zip(&path.point_nodes) {
            let mesh = graph.mesh(node.mesh);
            let triangle = mesh.triangles[node.triangle as usize]
                .vertices
                .map(|v| mesh.vertices[v as usize]);
            for t in [0.25, 0.5, 0.75] {
                let point = std::array::from_fn(|i| pair[0][i] * (1.0 - t) + pair[1][i] * t);
                assert!(
                    on_triangle(point, triangle),
                    "route {mi}, {node:?}, leaves surface at {point:?}"
                );
            }
        }
        sampled += 1;
        shortened += usize::from(after + 0.01 < before);
        if sampled == 100 {
            break;
        }
    }
    assert!(sampled >= 50, "only {sampled} connected non-gap routes");
    assert!(
        shortened * 2 >= sampled,
        "only {shortened} of {sampled} routes shortened"
    );
    eprintln!("funnel survey: {sampled} surface-checked routes, {shortened} shortened in XY");

    // Reachability from the first triangle of the largest mesh: the region
    // should span many navmeshes, which is what stitching is for.
    let reach = graph.reachable(NodeId::new(largest as u32, 0));
    let meshes_reached = reach
        .iter()
        .map(|n| n.mesh)
        .collect::<std::collections::HashSet<_>>()
        .len();
    eprintln!(
        "reachable from the largest navmesh: {} triangles across {meshes_reached} navmeshes",
        reach.len()
    );
    assert!(meshes_reached > 100, "{meshes_reached} navmeshes reached");

    // Door links count against the parsed records, so the stats match the
    // views.
    let doors: usize = order
        .winners_of(tag::NAVM)
        .filter(|(_, r)| r.placement.world == Some(tamriel_raw))
        .filter_map(|(_, r)| NavMesh::from_record(r).ok())
        .map(|n| n.geometry.map(|g| g.door_links.len()).unwrap_or(0))
        .sum();
    assert_eq!(doors, stats.door_links);
}

fn on_triangle(point: [f32; 3], triangle: [[f32; 3]; 3]) -> bool {
    let [a, b, c] = triangle.map(|p| p.map(f64::from));
    let p = point.map(f64::from);
    let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if determinant.abs() < 1e-12 {
        return false;
    }
    let u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / determinant;
    let v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / determinant;
    let w = 1.0 - u - v;
    u >= -0.001
        && v >= -0.001
        && w >= -0.001
        && (p[2] - u * a[2] - v * b[2] - w * c[2]).abs() < 0.25
}
