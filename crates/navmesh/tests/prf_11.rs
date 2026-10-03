// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-11 installed point-to-point timings on authored NAVM graphs.

use soe_esm::{tag, LoadOrder, Plugin};
use soe_navmesh::{vanilla, NodeId};

#[test]
fn prf_11_installed_bannered_mare_and_whiterun_path_timings() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let data = std::path::PathBuf::from(data);
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    for (name, kind) in [
        ("WhiterunBanneredMare", tag::CELL),
        ("WhiterunWorld", tag::WRLD),
    ] {
        let id = order
            .winners_of(kind)
            .find(|(_, record)| record.editor_id().as_deref() == Some(name))
            .map(|(id, _)| id)
            .unwrap();
        let raw = order.record(order.overrides(id)[0]).form_id;
        let (graph, stats) = vanilla::from_load_order(&order, |record| {
            if kind == tag::CELL {
                record.placement.cell == Some(raw)
            } else {
                record.placement.world == Some(raw)
            }
        });
        let (mesh, geometry) = graph
            .meshes()
            .iter()
            .enumerate()
            .max_by_key(|(_, mesh)| mesh.triangles.len())
            .unwrap();
        let from = graph.centroid(NodeId::new(mesh as u32, 0));
        let to = graph.centroid(NodeId::new(
            mesh as u32,
            geometry.triangles.len() as u32 - 1,
        ));
        let start = std::time::Instant::now();
        let path = graph.find_path_between(from, to).unwrap();
        let cold = start.elapsed();
        let (nodes, cost) = if kind == tag::CELL {
            (30, 1914.888)
        } else {
            (55, 2723.3533)
        };
        assert_eq!(path.nodes.len(), nodes, "baseline corridor changed");
        assert!((path.cost - cost).abs() < 0.001, "baseline cost changed");

        let start = std::time::Instant::now();
        assert_eq!(graph.find_path_between(from, to), Some(path.clone()));
        let warm = start.elapsed();
        eprintln!(
            "PRF-11 {name}: meshes={} triangles={} from={from:?} to={to:?} nodes={} cost={} first={cold:?} warm={warm:?}",
            stats.meshes, stats.triangles, path.nodes.len(), path.cost,
        );
    }
}
