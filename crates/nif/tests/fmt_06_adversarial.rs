// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{NifWriter, TestVertex, IDENTITY};
use soe_nif::blocks::Block;
use soe_nif::parse;

#[test]
fn fmt_06_mesh_walker_keeps_all_uv_sets_and_first_set_alias() {
    let mut writer = NifWriter::new(100);
    let vertices = [
        TestVertex::at([0.0, 0.0, 0.0], [0.0, 0.0]),
        TestVertex::at([1.0, 0.0, 0.0], [1.0, 0.0]),
        TestVertex::at([0.0, 1.0, 0.0], [0.0, 1.0]),
    ];
    let geometry = writer.geometry_data(&vertices, false, &[[0, 1, 2]], None);
    let data = writer.add("NiTriShapeData", geometry);
    let shape_data = writer.ni_tri_shape("TwoUV", data, -1, -1, -1);
    let shape = writer.add("NiTriShape", shape_data);
    let root_data = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    let root = writer.add("NiNode", root_data);
    writer.roots = vec![root];

    let mut file = parse(&writer.finish()).expect("single-set source geometry parses");
    let second_uvs = [[0.125, 0.25], [0.5, 0.75], [0.875, 1.0]];
    let data = file
        .blocks
        .iter_mut()
        .find_map(|block| match block {
            Block::GeometryData(data) => Some(data),
            _ => None,
        })
        .expect("geometry block exists");
    data.uv_sets.push(second_uvs.to_vec());
    let mesh = file.meshes().unwrap().remove(0);
    assert_eq!(mesh.uv_sets.len(), 2);
    assert_eq!(mesh.uv_sets[0], mesh.uvs);
    assert_eq!(mesh.uv_sets[1], second_uvs);
}
