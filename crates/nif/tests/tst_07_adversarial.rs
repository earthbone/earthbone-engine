// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_nif::blocks::Block;

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn export_string(out: &mut Vec<u8>, value: &str) {
    out.push(value.len() as u8 + 1);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}
fn object_net(out: &mut Vec<u8>, name: u32) {
    put_u32(out, name);
    put_u32(out, 0);
    put_i32(out, -1);
}
fn av_object(out: &mut Vec<u8>, name: u32) {
    object_net(out, name);
    put_u32(out, 0xE);
    for value in [0.0, 0.0, 0.0] {
        put_f32(out, value);
    }
    for value in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        put_f32(out, value);
    }
    put_f32(out, 1.0);
    put_i32(out, -1);
}

fn generated_nif() -> Vec<u8> {
    let mut node = Vec::new();
    av_object(&mut node, 0);
    put_u32(&mut node, 1);
    put_i32(&mut node, 1);
    put_u32(&mut node, 0);

    let mut shape = Vec::new();
    av_object(&mut shape, 1);
    for value in [0.0, 0.0, 0.0, 1.0] {
        put_f32(&mut shape, value);
    }
    for _ in 0..3 {
        put_i32(&mut shape, -1);
    }
    // Vertex, UV, normal, tangent and color attributes in stream 100.
    shape.extend_from_slice(&soe_nif::vertex::VertexDesc::from_attributes(
        soe_nif::vertex::attr::VERTEX
            | soe_nif::vertex::attr::UVS
            | soe_nif::vertex::attr::NORMALS
            | soe_nif::vertex::attr::TANGENTS
            | soe_nif::vertex::attr::COLORS,
        100,
    ).0.to_le_bytes());
    put_u16(&mut shape, 1);
    put_u16(&mut shape, 3);
    put_u32(&mut shape, 102);
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)] {
        for value in [x, y, 0.0] {
            put_f32(&mut shape, value);
        }
        put_u32(&mut shape, 0);
        for value in [0u16, 65535] {
            put_u16(&mut shape, value);
        }
        shape.extend_from_slice(&[127, 127, 255, 127, 127, 127, 0, 127]);
        shape.extend_from_slice(&[255; 4]);
    }
    for value in [0u16, 1, 2] {
        put_u16(&mut shape, value);
    }
    put_u32(&mut shape, 0);

    let blocks = [node, shape];
    let mut out = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
    put_u32(&mut out, 0x1402_0007);
    out.push(1);
    put_u32(&mut out, 12);
    put_u32(&mut out, 2);
    put_u32(&mut out, 100);
    export_string(&mut out, "test");
    export_string(&mut out, "");
    export_string(&mut out, "");
    put_u16(&mut out, 2);
    for name in ["NiNode", "BSTriShape"] {
        put_u32(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }
    put_u16(&mut out, 0);
    put_u16(&mut out, 1);
    for block in &blocks {
        put_u32(&mut out, block.len() as u32);
    }
    put_u32(&mut out, 2);
    put_u32(&mut out, 10);
    for name in ["Root", "Triangle"] {
        put_u32(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }
    put_u32(&mut out, 0);
    for block in blocks {
        out.extend(block);
    }
    put_u32(&mut out, 1);
    put_i32(&mut out, 0);
    out
}

#[test]
fn complete_generated_scene_reaches_both_decoders_and_mesh_output() {
    let file = soe_nif::parse(&generated_nif()).unwrap();
    assert!(matches!(file.blocks.first(), Some(Block::Node(_))));
    assert!(matches!(file.blocks.get(1), Some(Block::BsTriShape(_))));
    let meshes = file.meshes().unwrap();
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].vertex_count(), 3);
    assert_eq!(meshes[0].indices, vec![0, 1, 2]);
}

#[test]
fn invalid_triangle_vertex_index_is_rejected() {
    let mut bytes = generated_nif();
    let offset = bytes
        .windows(6)
        .rposition(|window| window == [0, 0, 1, 0, 2, 0])
        .unwrap();
    bytes[offset + 4..offset + 6].copy_from_slice(&3u16.to_le_bytes());
    assert!(soe_nif::parse(&bytes).is_err());
}

#[test]
fn root_reference_outside_block_table_is_rejected() {
    let mut bytes = generated_nif();
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&7i32.to_le_bytes());
    assert!(soe_nif::parse(&bytes).is_err());
}
