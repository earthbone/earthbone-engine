// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! BSDynamicTriShape fixtures follow nif.xml's Vector4 tail and its byte size.

mod common;

use common::{Buf, NifWriter, TestVertex};
use soe_nif::blocks::Block;
use soe_nif::vertex::attr;
use soe_nif::{parse, Error};

fn dynamic_shape(dynamic_data_size: u32, positions: &[[f32; 4]]) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let vertices = [
        TestVertex::at([0.0, 0.0, 0.0], [0.0, 0.0]),
        TestVertex::at([1.0, 0.0, 0.0], [1.0, 0.0]),
    ];
    let mut shape = Buf(writer.bs_tri_shape(
        "Dynamic",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX,
        &vertices,
        &[],
        false,
    ));
    shape.u32(dynamic_data_size);
    for position in positions {
        shape.floats(position);
    }
    writer.add("BSDynamicTriShape", shape.0);
    writer.finish()
}

#[test]
fn fmt_19_dynamic_shape_exposes_full_precision_positions_for_each_vertex() {
    let positions = [[1.25, -2.5, 3.75, 1.0], [-4.0, 5.5, 6.25, 0.5]];
    let file = parse(&dynamic_shape(32, &positions)).expect("the dynamic shape parses");
    let Block::BsTriShape(shape) = &file.blocks[0] else {
        panic!("the dynamic shape uses the BSTriShape block model");
    };

    assert_eq!(shape.num_vertices, 2);
    assert_eq!(shape.dynamic_positions, positions);
}

#[test]
fn fmt_19_dynamic_shape_rejects_size_that_disagrees_with_vertex_count() {
    let error = parse(&dynamic_shape(16, &[[1.0, 2.0, 3.0, 1.0]; 2])).unwrap_err();

    assert!(matches!(error, Error::InvalidData { block: 0, .. }));
}

#[test]
fn fmt_19_dynamic_shape_rejects_truncated_position_array() {
    let error = parse(&dynamic_shape(32, &[[1.0, 2.0, 3.0, 1.0]])).unwrap_err();

    assert!(matches!(error, Error::CountTooLarge { .. }));
}
