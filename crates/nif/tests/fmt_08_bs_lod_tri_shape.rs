// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! FMT-08 fixtures for the three BSLODTriShape LOD size fields.

mod common;

use common::{Buf, NifWriter, IDENTITY};
use soe_nif::blocks::Block;
use soe_nif::parse;

#[test]
fn fmt_08_bs_lod_tri_shape_preserves_each_u32_lod_size() {
    let mut writer = NifWriter::new(100);
    let mut block = Buf(writer.ni_tri_shape("Distant:0", -1, -1, -1, -1));
    let authored = [70_000, 0x1234_5678, u32::MAX];
    for value in authored {
        block.u32(value);
    }
    let shape = writer.add("BSLODTriShape", block.0);
    let root = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", root)];

    let file = parse(&writer.finish()).expect("the authored LOD shape parses");
    let parsed = file
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::TriShape(shape) if shape.av.name() == "Distant:0" => Some(shape),
            _ => None,
        })
        .expect("the LOD shape is present");

    assert_eq!(parsed.lod_sizes, Some(authored));
}

#[test]
fn fmt_08_regular_tri_shape_has_no_lod_sizes() {
    let mut writer = NifWriter::new(100);
    let shape = writer.ni_tri_shape("Near:0", -1, -1, -1, -1);
    let shape = writer.add("NiTriShape", shape);
    let root = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    writer.roots = vec![writer.add("NiNode", root)];

    let file = parse(&writer.finish()).expect("the regular shape parses");
    let parsed = file
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::TriShape(shape) if shape.av.name() == "Near:0" => Some(shape),
            _ => None,
        })
        .expect("the regular shape is present");

    assert_eq!(parsed.lod_sizes, None);
}
