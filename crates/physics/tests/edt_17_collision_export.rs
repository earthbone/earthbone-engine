// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::parse;
use soe_physics::export::{export_collision_mesh, CollisionMesh};
use soe_physics::{collect, Layer, Shape, UNITS_PER_METER};

fn cube(name: &str) -> CollisionMesh {
    CollisionMesh {
        name: name.into(),
        positions: vec![
            [-UNITS_PER_METER, -UNITS_PER_METER, -UNITS_PER_METER],
            [UNITS_PER_METER, -UNITS_PER_METER, -UNITS_PER_METER],
            [-UNITS_PER_METER, UNITS_PER_METER, -UNITS_PER_METER],
            [-UNITS_PER_METER, -UNITS_PER_METER, UNITS_PER_METER],
        ],
    }
}

#[test]
fn edt_17_collision_suffix_exports_a_scaled_supported_shape_that_physics_loads() {
    let file = export_collision_mesh(&cube("crate_collision"))
        .unwrap()
        .unwrap();
    let collision = collect(&file).unwrap();
    assert_eq!(collision.colliders.len(), 1);
    let body = &collision.colliders[0];
    assert_eq!(body.node_name, "crate_collision");
    assert_eq!(body.layer, Layer::STATIC);
    assert_eq!(body.mass, 0.0);
    let Shape::Material { id: 1, shape } = &body.shape else {
        panic!("exported convex shape must retain its Havok material")
    };
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("exported bhkConvexVerticesShape must load as a convex hull")
    };
    assert_eq!(points.len(), 4);
    assert_eq!(
        points[0],
        soe_physics::Vec3::new(-UNITS_PER_METER, -UNITS_PER_METER, -UNITS_PER_METER)
    );
    assert_eq!(
        points[3],
        soe_physics::Vec3::new(-UNITS_PER_METER, -UNITS_PER_METER, UNITS_PER_METER)
    );
}

#[test]
fn edt_17_unmarked_mesh_is_not_exported_as_collision() {
    assert!(export_collision_mesh(&cube("crate")).unwrap().is_none());
    assert!(export_collision_mesh(&cube("crate_COLLISION"))
        .unwrap()
        .is_some());
}

#[test]
fn edt_17_invalid_collision_vertices_are_rejected() {
    let mut mesh = cube("bad_collision");
    mesh.positions[0][0] = f32::NAN;
    assert!(export_collision_mesh(&mesh).is_err());

    mesh = cube("few_collision");
    mesh.positions.truncate(3);
    assert!(export_collision_mesh(&mesh).is_err());
}

#[test]
fn edt_17_collision_convention_writes_nif_bytes_that_reload_into_physics() {
    let source = cube("crate_collision");
    let exported = export_collision_mesh(&source).unwrap().unwrap();
    let bytes = exported.to_bytes().unwrap();
    let parsed = parse(&bytes).unwrap();

    assert_eq!(parsed.header.bs_version, soe_nif::header::BS_SKYRIM_SE);
    assert_eq!(
        parsed.node_name(parsed.roots[0]).as_deref(),
        Some("crate_collision")
    );
    assert!(parsed.block_types().contains(&"bhkRigidBodyT"));
    assert!(parsed.block_types().contains(&"bhkConvexVerticesShape"));

    let collision = collect(&parsed).unwrap();
    assert_eq!(collision.colliders.len(), 1);
    let body = &collision.colliders[0];
    assert_eq!(body.layer, Layer::STATIC);
    assert_eq!(body.mass, 0.0);
    let Shape::Material { id: 1, shape } = &body.shape else {
        panic!("serialized convex shape must retain its Havok material")
    };
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("serialized collision must reload as a supported convex hull")
    };
    assert_eq!(points.len(), source.positions.len());
    assert_eq!(
        points[0],
        soe_physics::Vec3::new(-UNITS_PER_METER, -UNITS_PER_METER, -UNITS_PER_METER)
    );
    assert_eq!(
        points[3],
        soe_physics::Vec3::new(-UNITS_PER_METER, -UNITS_PER_METER, UNITS_PER_METER)
    );
}
