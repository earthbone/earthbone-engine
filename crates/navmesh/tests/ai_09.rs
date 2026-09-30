// SPDX-License-Identifier: GPL-3.0-or-later

//! NAVM door associations retain the named triangle without treating it as an
//! ordinary graph edge.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::records::navm::{DoorLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{tag, LoadOrder, Plugin};
use soe_navmesh::vanilla::{self, Builder};
use soe_navmesh::NodeId;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

fn geometry(door_links: Vec<DoorLink>) -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(0x100),
        vertices: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]],
        triangles: vec![Triangle {
            vertices: [0, 1, 2],
            edges: [-1; 3],
            flags: 0,
            cover_flags: 0,
        }],
        edge_links: vec![],
        door_links,
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [100.0, 100.0],
            min: [0.0, 0.0, 0.0],
            max: [100.0, 100.0, 0.0],
            cells: vec![vec![0]],
        },
    }
}

#[test]
fn ai_09_resolves_each_door_reference_to_its_authored_triangle() {
    let mut builder = Builder::new();
    builder.add(
        0x1000,
        &geometry(vec![
            DoorLink {
                triangle: 0,
                crc: 0xABCD,
                door: 0x200,
            },
            DoorLink {
                triangle: 0,
                crc: 0xABCD,
                door: 0x201,
            },
        ]),
        |form| Some(u64::from(form) + 0x10000),
    );
    let (graph, _) = builder.build();

    assert_eq!(graph.door_nodes(0x10200), &[NodeId::new(0, 0)]);
    assert_eq!(graph.door_nodes(0x10201), &[NodeId::new(0, 0)]);
}

#[test]
fn ai_09_door_triangle_association_does_not_block_walkability() {
    let mut builder = Builder::new();
    builder.add(
        0x1000,
        &geometry(vec![DoorLink {
            triangle: 0,
            crc: 0xABCD,
            door: 0x200,
        }]),
        |form| Some(u64::from(form)),
    );
    let (graph, _) = builder.build();
    let door_triangle = NodeId::new(0, 0);

    assert_eq!(graph.door_nodes(0x200), &[door_triangle]);
    assert!(!graph.triangle(door_triangle).blocked);
}

#[test]
fn ai_09_ignores_unresolved_door_references_and_invalid_triangles() {
    let mut builder = Builder::new();
    builder.add(
        0x1000,
        &geometry(vec![
            DoorLink {
                triangle: 0,
                crc: 0xABCD,
                door: 0x200,
            },
            DoorLink {
                triangle: -1,
                crc: 0xABCD,
                door: 0x201,
            },
        ]),
        |form| (form == 0x201).then_some(u64::from(form)),
    );
    let (graph, _) = builder.build();

    assert!(graph.door_nodes(0x200).is_empty());
    assert!(graph.door_nodes(0x201).is_empty());
}

#[test]
fn ai_09_builder_still_counts_authored_door_links() {
    let mut builder = Builder::new();
    builder.add(
        0x1000,
        &geometry(vec![DoorLink {
            triangle: 0,
            crc: 0xABCD,
            door: 0x200,
        }]),
        |form| Some(u64::from(form)),
    );
    let (_, stats) = builder.build();

    assert_eq!(stats.door_links, 1);
}

#[test]
fn ai_09_installed_navmeshes_resolve_door_associations_to_valid_triangles() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        order.add(name, plugin).unwrap();
    }

    let (graph, stats) = vanilla::from_load_order(&order, |_| true);
    assert!(
        stats.door_links > 0,
        "the base masters contain no NAVM door links"
    );
    let mut resolved = 0;
    for (navmesh, _) in order.winners_of(tag::NAVM) {
        let Some(reference) = order.winner_ref(navmesh) else {
            continue;
        };
        let record = order.record(reference);
        let Ok(parsed) = soe_esm::records::navm::NavMesh::from_record(record) else {
            continue;
        };
        let Some(geometry) = parsed.geometry else {
            continue;
        };
        for link in geometry.door_links {
            let door = order.resolve(reference.file, link.door);
            let nodes = graph.door_nodes(vanilla::key_of(door));
            if nodes.is_empty() {
                continue;
            }
            assert!(nodes.iter().all(|node| {
                graph
                    .meshes()
                    .get(node.mesh as usize)
                    .is_some_and(|mesh| (node.triangle as usize) < mesh.triangles.len())
            }));
            resolved += 1;
        }
    }
    assert!(resolved > 0, "no installed NAVM door association resolved");
}
