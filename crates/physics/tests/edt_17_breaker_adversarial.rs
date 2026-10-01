// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::blocks::Block;
use soe_nif::parse;
use soe_physics::export::{export_collision_mesh, CollisionMesh};
use soe_physics::{collect, Shape, UNITS_PER_METER};

fn mesh(name: &str, positions: Vec<[f32; 3]>) -> CollisionMesh {
    CollisionMesh {
        name: name.into(),
        positions,
    }
}

fn write_and_reload(input: &CollisionMesh) -> soe_nif::NifFile {
    let exported = export_collision_mesh(input).unwrap().unwrap();
    parse(&exported.to_bytes().unwrap()).unwrap()
}

#[test]
fn byte_round_trip_preserves_asymmetric_game_unit_positions() {
    let input = mesh(
        "asymmetric_collision",
        vec![
            [-137.25, 29.5, 81.75],
            [46.0, 116.5, -9.25],
            [0.0, 0.0, 0.0],
            [203.75, -52.25, 137.0],
        ],
    );
    let parsed = write_and_reload(&input);
    let collision = collect(&parsed).unwrap();
    let Shape::Material { shape, .. } = &collision.colliders[0].shape else {
        panic!("reloaded collision should retain its material wrapper");
    };
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("reloaded collision should be a convex hull");
    };
    assert_eq!(points.len(), input.positions.len());
    for (actual, expected) in points.iter().zip(&input.positions) {
        assert!((actual.x - expected[0]).abs() < 0.001);
        assert!((actual.y - expected[1]).abs() < 0.001);
        assert!((actual.z - expected[2]).abs() < 0.001);
    }
}

#[test]
fn convex_vertices_follow_the_nif_lexicographic_order() {
    let input = mesh(
        "ordered_collision",
        vec![
            [UNITS_PER_METER, 0.0, 0.0],
            [0.0, UNITS_PER_METER, 0.0],
            [0.0, 0.0, UNITS_PER_METER],
            [0.0, 0.0, 0.0],
        ],
    );
    let parsed = write_and_reload(&input);
    let collision = collect(&parsed).unwrap();
    let Shape::Material { shape, .. } = &collision.colliders[0].shape else {
        panic!("reloaded collision should retain its material wrapper");
    };
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("reloaded collision should be a convex hull");
    };
    let expected = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, UNITS_PER_METER],
        [0.0, UNITS_PER_METER, 0.0],
        [UNITS_PER_METER, 0.0, 0.0],
    ];
    for (actual, expected) in points.iter().zip(expected) {
        assert!((actual.x - expected[0]).abs() < 0.001);
        assert!((actual.y - expected[1]).abs() < 0.001);
        assert!((actual.z - expected[2]).abs() < 0.001);
    }
}

#[test]
fn convex_shape_properties_use_the_nif_capacity_flags_default() {
    let input = mesh(
        "property_collision",
        vec![
            [0.0, 0.0, 0.0],
            [UNITS_PER_METER, 0.0, 0.0],
            [0.0, UNITS_PER_METER, 0.0],
            [0.0, 0.0, UNITS_PER_METER],
        ],
    );
    let parsed = write_and_reload(&input);
    let Block::Collision { data, .. } = &parsed.blocks[3] else {
        panic!("convex vertices block should remain opaque collision data");
    };
    let word = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    assert_eq!(word(16), 0x8000_0000);
    assert_eq!(word(28), 0x8000_0000);
}
