// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Buf, NifWriter};
use soe_nif::blocks::Block;
use soe_nif::{parse, Error};

#[test]
fn fmt_19_zero_vertex_dynamic_shape_accepts_zero_length_tail() {
    let mut writer = NifWriter::new(100);
    let mut data = Buf(writer.bs_tri_shape("Empty", [0.0; 3], -1, -1, -1, 0, &[], &[], false));
    data.u32(0);
    writer.add("BSDynamicTriShape", data.0);

    let file = parse(&writer.finish()).expect("zero-vertex dynamic shape is valid");
    let Block::BsTriShape(shape) = &file.blocks[0] else { panic!("dynamic shape block") };
    assert_eq!(shape.num_vertices, 0);
    assert!(shape.dynamic_positions.is_empty());
}

#[test]
fn fmt_19_dynamic_tail_with_extra_vector_is_rejected_by_size() {
    let mut writer = NifWriter::new(100);
    let mut data = Buf(writer.bs_tri_shape("Extra", [0.0; 3], -1, -1, -1, 0, &[], &[], false));
    data.u32(16).floats(&[1.0, 2.0, 3.0, 4.0]);
    writer.add("BSDynamicTriShape", data.0);

    assert!(matches!(parse(&writer.finish()), Err(Error::InvalidData { block: 0, .. })));
}
