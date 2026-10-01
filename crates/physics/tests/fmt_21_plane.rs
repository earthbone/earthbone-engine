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

    fn vec4(&mut self, value: [f32; 3]) {
        for component in value {
            self.f32(component);
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

fn plane_data() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.u32(7);
    writer.zeros(12);
    writer.f32(0.0);
    writer.f32(0.0);
    writer.f32(1.0);
    writer.f32(2.0);
    writer.vec4([3.0, 4.0, 5.0]);
    writer.vec4([0.0, 0.0, 0.0]);
    writer.0
}

fn fixture_blocks() -> Vec<Block> {
    let mut node = Node::default();
    node.av.net.name = "PlaneFixture".into();
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
            type_name: "bhkPlaneShape".into(),
            data: plane_data(),
        },
    ]
}

#[test]
fn fmt_21_plane_fixture_preserves_normal_and_distance_after_unit_conversion() {
    let data = plane_data();
    assert!(
        matches!(
            bhk::decode("bhkPlaneShape", &data, &header()).unwrap(),
            Bhk::Plane {
                material: 7,
                normal,
                distance: 2.0,
                half_extents,
                center,
            } if normal == Vec3::new(0.0, 0.0, 1.0)
                && half_extents == Vec3::new(3.0, 4.0, 5.0)
                && center == Vec3::ZERO
        ),
        "plane fields differ from the synthetic payload"
    );

    let collected = collect_blocks(&header(), &fixture_blocks(), &[BlockRef(0)]).unwrap();
    assert!(collected.skipped.is_empty(), "{:?}", collected.skipped);
    let collider = &collected.colliders[0];
    let Shape::Material { id: 7, shape } = &collider.shape else {
        panic!("plane material was not retained");
    };
    let Shape::TriangleMesh {
        vertices,
        triangles,
        ..
    } = shape.as_ref()
    else {
        panic!("bounded plane was not converted to a triangle mesh");
    };
    assert_eq!(triangles.len(), 2);
    for point in vertices {
        assert!(
            (point.z - 2.0 * UNITS_PER_METER).abs() < 0.001,
            "vertex {point:?} does not preserve the plane distance"
        );
    }
    let [a, b, c] = triangles[0].map(|index| vertices[index as usize]);
    assert!((b - a).cross(c - a).normalized().z > 0.999);
}

#[test]
fn fmt_21_plane_decoder_rejects_truncated_and_invalid_geometry() {
    let data = plane_data();
    for length in 0..data.len() {
        assert!(
            bhk::decode("bhkPlaneShape", &data[..length], &header()).is_err(),
            "accepted truncated plane payload of {length} bytes"
        );
    }

    for (offset, value) in [(16, f32::NAN), (28, f32::INFINITY), (32, -1.0)] {
        let mut invalid = data.clone();
        invalid[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            bhk::decode("bhkPlaneShape", &invalid, &header()).is_err(),
            "accepted invalid plane field at byte {offset}"
        );
    }
}
