// SPDX-License-Identifier: GPL-3.0-or-later

//! A bounded Skyrim SE NIF writer for the structure-aware fuzz target.
//!
//! The header, block table, string table, references and packed vertex fields
//! follow nifxml's `nif.xml` definitions for stream 100.

use arbitrary::Arbitrary;
use soe_nif::vertex::{attr, VertexDesc};

const BS_VERSION: u32 = 100;
const NIF_VERSION: u32 = 0x1402_0007;
const ATTRIBUTES: u16 = attr::VERTEX | attr::UVS | attr::NORMALS | attr::TANGENTS | attr::COLORS;

#[derive(Arbitrary, Debug)]
pub struct Plan {
    positions: [i16; 9],
    uvs: [u16; 6],
    normals: [u8; 24],
    colors: [u8; 12],
    node_flags: u16,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            positions: [0, 0, 0, 1024, 0, 0, 0, 1024, 0],
            uvs: [0, 0, 65535, 0, 0, 65535],
            normals: [
                127, 127, 255, 127, 127, 127, 0, 127, 127, 127, 255, 127, 127, 127, 0, 127, 127,
                127, 255, 127, 127, 127, 0, 127,
            ],
            colors: [255; 12],
            node_flags: 0xE,
        }
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_export_string(out: &mut Vec<u8>, value: &str) {
    out.push(value.len() as u8 + 1);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}

fn object_net(out: &mut Vec<u8>, name_index: u32) {
    put_u32(out, name_index);
    put_u32(out, 0);
    put_i32(out, -1);
}

fn av_object(out: &mut Vec<u8>, name_index: u32, flags: u32) {
    object_net(out, name_index);
    put_u32(out, flags);
    for value in [0.0, 0.0, 0.0] {
        put_f32(out, value);
    }
    for value in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        put_f32(out, value);
    }
    put_f32(out, 1.0);
    put_i32(out, -1);
}

fn node_block(plan: &Plan) -> Vec<u8> {
    let mut out = Vec::new();
    av_object(&mut out, 0, u32::from(plan.node_flags));
    put_u32(&mut out, 1);
    put_i32(&mut out, 1);
    put_u32(&mut out, 0);
    out
}

fn shape_block(plan: &Plan) -> Vec<u8> {
    let mut out = Vec::new();
    av_object(&mut out, 1, 0xE);
    for value in [0.0, 0.0, 0.0, 1.0] {
        put_f32(&mut out, value);
    }
    for reference in [-1, -1, -1] {
        put_i32(&mut out, reference);
    }
    put_u64(
        &mut out,
        VertexDesc::from_attributes(ATTRIBUTES, BS_VERSION).0,
    );
    put_u16(&mut out, 1);
    put_u16(&mut out, 3);
    put_u32(&mut out, 3 * 32 + 6);
    for vertex in 0..3 {
        for axis in 0..3 {
            let coordinate = f32::from(plan.positions[vertex * 3 + axis]) / 1024.0;
            put_f32(&mut out, coordinate);
        }
        put_u32(&mut out, 0);
        for axis in 0..2 {
            put_u16(&mut out, plan.uvs[vertex * 2 + axis]);
        }
        let normal_at = vertex * 8;
        out.extend_from_slice(&plan.normals[normal_at..normal_at + 8]);
        let color_at = vertex * 4;
        out.extend_from_slice(&plan.colors[color_at..color_at + 4]);
    }
    for index in [0, 1, 2] {
        put_u16(&mut out, index);
    }
    put_u32(&mut out, 0);
    out
}

/// Build a complete stream-100 file containing a node and one packed triangle.
pub fn build(plan: &Plan) -> Vec<u8> {
    let blocks = [node_block(plan), shape_block(plan)];
    let mut out = Vec::new();
    out.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    put_u32(&mut out, NIF_VERSION);
    out.push(1);
    put_u32(&mut out, 12);
    put_u32(&mut out, blocks.len() as u32);
    put_u32(&mut out, BS_VERSION);
    put_export_string(&mut out, "soe-nif grammar");
    put_export_string(&mut out, "");
    put_export_string(&mut out, "");
    put_u16(&mut out, 2);
    for type_name in ["NiNode", "BSTriShape"] {
        put_u32(&mut out, type_name.len() as u32);
        out.extend_from_slice(type_name.as_bytes());
    }
    put_u16(&mut out, 0);
    put_u16(&mut out, 1);
    for block in &blocks {
        put_u32(&mut out, block.len() as u32);
    }
    put_u32(&mut out, 2);
    put_u32(&mut out, 10);
    for name in ["Scene Root", "Triangle"] {
        put_u32(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }
    put_u32(&mut out, 0);
    for block in blocks {
        out.extend_from_slice(&block);
    }
    put_u32(&mut out, 1);
    put_i32(&mut out, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_nif::blocks::Block;

    #[test]
    fn tst_07_generated_file_reaches_node_and_shape_decoders() {
        let bytes = build(&Plan::default());
        let file = soe_nif::parse(&bytes).unwrap();

        assert!(matches!(file.blocks.first(), Some(Block::Node(_))));
        assert!(matches!(file.blocks.get(1), Some(Block::BsTriShape(_))));
        assert_eq!(file.block_types(), ["NiNode", "BSTriShape"]);
    }

    #[test]
    fn tst_07_generated_triangle_reaches_mesh_decoder() {
        let file = soe_nif::parse(&build(&Plan::default())).unwrap();
        let meshes = file.meshes().unwrap();

        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].triangle_count(), 1);
    }
}
