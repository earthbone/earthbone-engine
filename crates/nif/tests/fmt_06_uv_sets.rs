// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{NifWriter, TestVertex, IDENTITY};
use soe_nif::blocks::Block;
use soe_nif::parse;

#[test]
fn fmt_06_mesh_exposes_every_uv_set_and_keeps_the_first_set_alias() {
    let mut writer = NifWriter::new(83);
    let vertices = vec![
        TestVertex::at([0.0, 0.0, 0.0], [0.0, 0.0]),
        TestVertex::at([1.0, 0.0, 0.0], [1.0, 0.0]),
        TestVertex::at([0.0, 1.0, 0.0], [0.0, 1.0]),
    ];
    let data = writer.geometry_data(&vertices, false, &[[0, 1, 2]], None);
    let data = writer.add("NiTriShapeData", data);
    let shape = writer.ni_tri_shape("Two UV sets", data, -1, -1, -1);
    let shape = writer.add("NiTriShape", shape);
    let root_data = writer.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    let root = writer.add("NiNode", root_data);
    writer.roots = vec![root];

    let mut file = parse(&writer.finish()).unwrap();
    let geometry = file
        .blocks
        .iter_mut()
        .find_map(|block| match block {
            Block::GeometryData(data) => Some(data),
            _ => None,
        })
        .expect("NiTriShapeData parsed");
    geometry
        .uv_sets
        .push(vec![[0.25, 0.5], [0.75, 0.5], [0.25, 1.0]]);

    let mesh = file.meshes().unwrap().remove(0);
    assert_eq!(mesh.uv_sets.len(), 2);
    assert_eq!(mesh.uv_sets[0], mesh.uvs);
    assert_eq!(mesh.uv_sets[1], vec![[0.25, 0.5], [0.75, 0.5], [0.25, 1.0]]);
    assert_eq!(mesh.approx_bytes(), 3 * (12 + 12 + 16) + 3 * 4);
}
