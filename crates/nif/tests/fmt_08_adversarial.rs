// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Buf, NifWriter, IDENTITY};
use soe_nif::blocks::Block;
use soe_nif::parse;

#[test]
fn fmt_08_lod_ranges_keep_zero_and_full_u32_values() {
    let mut writer = NifWriter::new(100);
    let mut shape_data = Buf(writer.ni_tri_shape("Boundary", -1, -1, -1, -1));
    let expected = [0, 1, u32::MAX];
    for value in expected {
        shape_data.u32(value);
    }
    let shape = writer.add("BSLODTriShape", shape_data.0);
    let root_data = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", root_data)];

    let file = parse(&writer.finish()).expect("LOD shape parses");
    let Block::TriShape(parsed) = &file.blocks[0] else { panic!("LOD shape block") };
    assert_eq!(parsed.lod_sizes, Some(expected));
}

#[test]
fn fmt_08_truncated_lod_range_is_rejected() {
    let mut writer = NifWriter::new(100);
    let mut shape_data = writer.ni_tri_shape("Truncated", -1, -1, -1, -1);
    shape_data.extend_from_slice(&12u32.to_le_bytes());
    let shape = writer.add("BSLODTriShape", shape_data);
    let root_data = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", root_data)];

    assert!(parse(&writer.finish()).is_err());
}
