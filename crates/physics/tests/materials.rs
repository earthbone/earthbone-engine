// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Surface identity must survive shape collection, placement and ray queries.
//! Material IDs here are Skyrim's wood, stone and dirt values from nif.xml.

use soe_nif::BlockRef;
use soe_physics::{Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3};

const WOOD: u32 = 500_811_281;
const STONE: u32 = 3_741_512_247;
const DIRT: u32 = 3_106_094_762;

fn collider(shape: Shape) -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: "material fixture".into(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: Iso::translation(Vec3::new(100.0, 200.0, 300.0)),
        shape,
    }
}

fn material(id: u32, shape: Shape) -> Shape {
    Shape::Material {
        id,
        shape: Box::new(shape),
    }
}

#[test]
fn mixed_faces_keep_materials_after_invalid_faces_are_skipped() {
    let shape = Shape::TriangleMesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(20.0, 0.0, 0.0),
            Vec3::new(20.0, 20.0, 0.0),
            Vec3::new(0.0, 20.0, 0.0),
        ],
        triangles: vec![[0, 1, 2], [0, 1, 99], [0, 2, 3]],
        materials: vec![Some(WOOD), Some(DIRT), Some(STONE)],
    };
    let mut world = SimpleWorld::new();
    let body = world.add_static(&collider(material(DIRT, shape)));
    for (x, y, expected) in [(15.0, 5.0, WOOD), (5.0, 15.0, STONE)] {
        let hit = world
            .raycast(
                Vec3::new(100.0 + x, 200.0 + y, 310.0),
                Vec3::new(100.0 + x, 200.0 + y, 290.0),
            )
            .unwrap();
        assert_eq!(hit.body, body);
        assert_eq!(hit.material, Some(expected));
        assert!((hit.point.z - 300.0).abs() < 0.001);
    }
}

#[test]
fn nested_materials_do_not_leak_to_sibling_shapes() {
    let child = |x, shape| (Iso::translation(Vec3::new(x, 0.0, 0.0)), shape);
    let shape = material(
        DIRT,
        Shape::Compound(vec![
            child(
                0.0,
                material(
                    WOOD,
                    Shape::Box {
                        half_extents: Vec3::new(2.0, 2.0, 2.0),
                    },
                ),
            ),
            child(10.0, Shape::Sphere { radius: 2.0 }),
            child(
                20.0,
                material(
                    STONE,
                    Shape::Capsule {
                        a: Vec3::ZERO,
                        b: Vec3::Z,
                        radius: 2.0,
                    },
                ),
            ),
            child(
                30.0,
                Shape::TriangleMesh {
                    vertices: vec![
                        Vec3::new(-2.0, -2.0, 0.0),
                        Vec3::new(2.0, -2.0, 0.0),
                        Vec3::new(0.0, 2.0, 0.0),
                    ],
                    triangles: vec![[0, 1, 2]],
                    materials: Vec::new(),
                },
            ),
        ]),
    );
    let mut world = SimpleWorld::new();
    world.add_static(&collider(shape));
    for (x, expected) in [(0.0, WOOD), (10.0, DIRT), (20.0, STONE), (30.0, DIRT)] {
        let hit = world
            .raycast(
                Vec3::new(100.0 + x, 200.0, 310.0),
                Vec3::new(100.0 + x, 200.0, 290.0),
            )
            .unwrap();
        assert_eq!(hit.material, Some(expected), "child at {x}");
    }
}

#[test]
fn unknown_material_stays_unknown() {
    let mut world = SimpleWorld::new();
    world.add_static(&collider(Shape::Sphere { radius: 2.0 }));
    let hit = world
        .raycast(
            Vec3::new(100.0, 200.0, 310.0),
            Vec3::new(100.0, 200.0, 290.0),
        )
        .unwrap();
    assert_eq!(hit.material, None);
}
