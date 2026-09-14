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
        stats.resolved_links + stats.unresolved_links
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
