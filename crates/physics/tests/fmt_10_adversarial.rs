// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_nif::blocks::{Block, Node};
use soe_nif::{header::VERSION_20_2_0_7, BlockRef, Header};
use soe_physics::bhk::{self, Bhk};
use soe_physics::{collect_blocks, Layer, Shape, UNITS_PER_METER};

fn header() -> Header {
    Header { version: VERSION_20_2_0_7, user_version: 12, bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(), process_script: String::new(), export_script: String::new(), max_filepath: String::new(),
        block_types: Vec::new(), block_type_index: Vec::new(), block_sizes: Vec::new(), strings: Vec::new(), groups: Vec::new() }
}

fn box_blocks() -> Vec<Block> {
    let mut node = Node::default();
    node.av.net.name = "Radius".into();
    node.av.collision = BlockRef(1);
    let mut co = Vec::new();
    co.extend_from_slice(&(-1i32).to_le_bytes());
    co.extend_from_slice(&1u16.to_le_bytes());
    co.extend_from_slice(&2i32.to_le_bytes());
    let mut body = Vec::new();
    body.extend_from_slice(&3i32.to_le_bytes());
    body.extend_from_slice(&[Layer::STATIC.0, 0, 0, 0]);
    body.extend_from_slice(&[0; 20 + 4 + 20]);
    for value in [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body.extend_from_slice(&[0; 48]);
    for value in [0.0f32, 0.0, 0.0, 0.0, 0.1, 0.05, 1.0, 1.0, 0.5, 0.0, 0.4, 104.4, 31.57, 0.15] {
        body.extend_from_slice(&value.to_le_bytes());
    }
    body.extend_from_slice(&[7, 1, 1, 1, 0, 0, 0, 0]);
    body.extend_from_slice(&[0; 12]);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    let mut shape = Vec::new();
    shape.extend_from_slice(&3u32.to_le_bytes());
    shape.extend_from_slice(&0.05f32.to_le_bytes());
    shape.extend_from_slice(&[0; 8]);
    for value in [1.0f32, 2.0, 3.0, 0.0] { shape.extend_from_slice(&value.to_le_bytes()); }
    vec![
        Block::Node(node),
        Block::Collision { type_name: "bhkCollisionObject".into(), data: co },
        Block::Collision { type_name: "bhkRigidBodyT".into(), data: body },
        Block::Collision { type_name: "bhkBoxShape".into(), data: shape },
    ]
}

#[test]
fn fmt_10_box_radius_expands_every_authored_half_extent() {
    let mut data = Vec::new();
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&0.05f32.to_le_bytes());
    data.extend_from_slice(&[0; 8]);
    for value in [1.0f32, 2.0, 3.0, 0.0] { data.extend_from_slice(&value.to_le_bytes()); }
    let Bhk::Box { radius, .. } = bhk::decode("bhkBoxShape", &data, &header()).unwrap() else { panic!() };
    assert!((radius * soe_physics::UNITS_PER_METER - 3.4995625).abs() < 1e-5);
    let collected = collect_blocks(&header(), &box_blocks(), &[BlockRef(0)]).unwrap();
    let Shape::Material { shape, .. } = &collected.colliders[0].shape else { panic!() };
    let Shape::Box { half_extents } = shape.as_ref() else { panic!() };
    for (actual, expected) in [
        (half_extents.x, 1.05 * UNITS_PER_METER),
        (half_extents.y, 2.05 * UNITS_PER_METER),
        (half_extents.z, 3.05 * UNITS_PER_METER),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
}
