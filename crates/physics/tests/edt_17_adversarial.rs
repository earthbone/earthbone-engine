// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_physics::export::{export_collision_mesh, CollisionMesh};
use soe_physics::{collect, Shape};

fn mesh(name: &str, count: usize) -> CollisionMesh {
    let positions = (0..count)
        .map(|index| [index as f32, (index % 7) as f32, (index % 13) as f32])
        .collect();
    CollisionMesh {
        name: name.into(),
        positions,
    }
}

#[test]
fn collision_suffix_requires_the_suffix_at_the_end() {
    assert!(export_collision_mesh(&mesh("rock_collision_extra", 4))
        .unwrap()
        .is_none());
    assert!(export_collision_mesh(&mesh("rock_COLLISION", 4))
        .unwrap()
        .is_some());
}

#[test]
fn collision_vertex_limit_accepts_1024_and_rejects_1025() {
    assert!(export_collision_mesh(&mesh("large_collision", 1024))
        .unwrap()
        .is_some());
    assert!(export_collision_mesh(&mesh("oversize_collision", 1025)).is_err());
}

#[test]
fn collision_export_preserves_asymmetric_points_after_physics_collection() {
    let mut input = mesh("asymmetric_collision", 4);
    input.positions = vec![
        [-140.0, 35.0, 70.0],
        [21.0, 98.0, -7.0],
        [0.0, 0.0, 0.0],
        [70.0, -70.0, 140.0],
    ];
    let exported = export_collision_mesh(&input).unwrap().unwrap();
    let colliders = collect(&exported).unwrap();
    let Shape::Material { shape, .. } = &colliders.colliders[0].shape else {
        panic!("expected the exported convex shape material wrapper");
    };
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("expected the exported bhk convex vertices shape");
    };
    assert_eq!(points.len(), 4);
    for (actual, expected) in points.iter().zip(&input.positions) {
        assert!((actual.x - expected[0]).abs() < 0.001);
        assert!((actual.y - expected[1]).abs() < 0.001);
        assert!((actual.z - expected[2]).abs() < 0.001);
    }
}

#[test]
fn too_few_or_non_finite_marked_vertices_are_rejected() {
    assert!(export_collision_mesh(&mesh("empty_collision", 0)).is_err());
    let mut invalid = mesh("nan_collision", 4);
    invalid.positions[2][1] = f32::INFINITY;
    assert!(export_collision_mesh(&invalid).is_err());
}
