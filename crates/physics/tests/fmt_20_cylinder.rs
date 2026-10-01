// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::blocks::{Block, Node};
use soe_nif::{BlockRef, Header};
use soe_physics::bhk::{self, Bhk};
use soe_physics::{collect_blocks, Layer, Shape, Vec3, UNITS_PER_METER};

struct Writer(Vec<u8>);

impl Writer {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn zeros(&mut self, count: usize) {
        self.0.extend(std::iter::repeat_n(0, count));
    }

    fn vec4(&mut self, point: [f32; 3]) {
        for value in point {
            self.f32(value);
        }
        self.f32(0.0);
    }
}

fn header() -> Header {
    Header {
        version: soe_nif::header::VERSION_20_2_0_7,
        user_version: 12,
        bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(),
        process_script: String::new(),
        export_script: String::new(),
        max_filepath: String::new(),
        block_types: Vec::new(),
        block_type_index: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        groups: Vec::new(),
    }
}

fn cylinder_data() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.u32(7);
    writer.f32(0.1);
    writer.zeros(8);
    writer.vec4([0.0, 0.0, -1.0]);
    writer.vec4([0.0, 0.0, 2.0]);
    writer.f32(0.5);
    writer.zeros(12);
    writer.0
}

fn fixture_blocks() -> Vec<Block> {
    let mut node = Node::default();
    node.av.net.name = "CylinderFixture".into();
    node.av.transform.scale = 1.0;
    node.av.collision = BlockRef(1);

    let mut collision_object = Writer::new();
    collision_object.i32(-1);
    collision_object.u16(1);
    collision_object.i32(2);

    let mut body = Writer::new();
    body.i32(3);
    body.u8(Layer::STATIC.0);
    body.u8(0);
    body.u16(0);
    body.zeros(20 + 4 + 20);
    body.vec4([0.0; 3]);
    body.f32(0.0);
    body.f32(0.0);
    body.f32(0.0);
    body.f32(1.0);
    body.vec4([0.0; 3]);
    body.vec4([0.0; 3]);
    body.zeros(48);
    body.vec4([0.0; 3]);
    body.f32(0.0);
    body.f32(0.1);
    body.f32(0.05);
    body.f32(1.0);
    body.f32(1.0);
    body.f32(0.5);
    body.f32(0.0);
    body.f32(0.4);
    body.f32(104.4);
    body.f32(31.57);
    body.f32(0.15);
    for value in [7, 1, 1, 1, 0, 0, 0, 0] {
        body.u8(value);
    }
    body.zeros(12);
    body.u32(0);
    body.u16(0);

    vec![
        Block::Node(node),
        Block::Collision {
            type_name: "bhkCollisionObject".into(),
            data: collision_object.0,
        },
        Block::Collision {
            type_name: "bhkRigidBodyT".into(),
            data: body.0,
        },
        Block::Collision {
            type_name: "bhkCylinderShape".into(),
            data: cylinder_data(),
        },
    ]
}

#[test]
fn fmt_20_cylinder_fixture_preserves_endpoints_and_radius() {
    let blocks = fixture_blocks();
    let Block::Collision { data, .. } = &blocks[3] else {
        panic!("cylinder fixture block is not a collision block");
    };
    assert!(
        matches!(
            bhk::decode("bhkCylinderShape", data, &header()).unwrap(),
            Bhk::Cylinder {
                material: 7,
                shell_radius: 0.1,
                radius: 0.5,
                first,
                second,
            } if first == Vec3::new(0.0, 0.0, -1.0) && second == Vec3::new(0.0, 0.0, 2.0)
        ),
        "decoded cylinder fields differ from the fixture"
    );

    let collected = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(collected.skipped.is_empty(), "{:?}", collected.skipped);
    let collider = &collected.colliders[0];
    let Shape::Material { id, shape } = &collider.shape else {
        panic!("cylinder material was not retained");
    };
    assert_eq!(*id, 7);
    let Shape::ConvexHull { points } = shape.as_ref() else {
        panic!("cylinder was not converted to the neutral convex hull shape");
    };
    let bounds = collider.aabb();
    let shell_radius = 0.6 * UNITS_PER_METER;
    assert!((bounds.min.z - (-UNITS_PER_METER)).abs() < 0.001);
    assert!((bounds.max.z - 2.0 * UNITS_PER_METER).abs() < 0.001);
    assert!((bounds.max.x - shell_radius).abs() < 0.001);
    assert!((bounds.min.x + shell_radius).abs() < 0.001);
    assert_eq!(points.len(), 64);
}

#[test]
fn fmt_20_cylinder_decoder_rejects_truncation_and_invalid_geometry() {
    let data = cylinder_data();
    for length in 0..data.len() {
        assert!(
            bhk::decode("bhkCylinderShape", &data[..length], &header()).is_err(),
            "accepted truncated cylinder payload of {length} bytes"
        );
    }

    for (offset, value) in [(16, f32::NAN), (48, 0.0), (32, f32::INFINITY)] {
        let mut invalid = data.clone();
        invalid[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            bhk::decode("bhkCylinderShape", &invalid, &header()).is_err(),
            "accepted invalid cylinder value at byte {offset}"
        );
    }
}
