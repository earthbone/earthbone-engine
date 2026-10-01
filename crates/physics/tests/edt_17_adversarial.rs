// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::parse;
use soe_physics::export::{export_collision_mesh, CollisionMesh};
use soe_physics::{collect, Shape};

fn tetrahedron(name: &str) -> CollisionMesh {
    CollisionMesh {
        name: name.into(),
        positions: vec![
            [-139.75, -69.0, -34.25],
            [139.75, -69.0, -34.25],
            [0.0, 139.75, -34.25],
            [0.0, 0.0, 104.5],
        ],
    }
}

#[test]
fn mixed_case_collision_suffix_writes_a_physics_shape() {
    let exported = export_collision_mesh(&tetrahedron("crate_CoLlIsIoN"))
        .unwrap()
        .expect("ASCII case-insensitive suffix declares collision");
    let parsed = parse(&exported.to_bytes().unwrap()).unwrap();
    let collision = collect(&parsed).unwrap();
    assert_eq!(collision.colliders.len(), 1);
    assert_eq!(collision.colliders[0].node_name, "crate_CoLlIsIoN");
    let Shape::Material { shape, .. } = &collision.colliders[0].shape else {
        panic!("convex collision retains its material wrapper");
    };
    assert!(matches!(shape.as_ref(), Shape::ConvexHull { points } if points.len() == 4));
}

#[test]
fn windows_1252_node_name_survives_nif_byte_round_trip() {
    let exported = export_collision_mesh(&tetrahedron("café_collision"))
        .unwrap()
        .unwrap();
    let parsed = parse(&exported.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.node_name(parsed.roots[0]).as_deref(), Some("café_collision"));
    let collision = collect(&parsed).unwrap();
    assert_eq!(collision.colliders[0].node_name, "café_collision");
}

#[test]
fn repeated_nif_writes_are_stable_and_leave_the_graph_loadable() {
    let exported = export_collision_mesh(&tetrahedron("repeat_collision"))
        .unwrap()
        .unwrap();
    let first = exported.to_bytes().unwrap();
    let second = exported.to_bytes().unwrap();
    assert_eq!(first, second);
    let parsed = parse(&second).unwrap();
    let collision = collect(&parsed).unwrap();
    assert_eq!(collision.colliders.len(), 1);
    assert_eq!(collision.colliders[0].node_name, "repeat_collision");
}
