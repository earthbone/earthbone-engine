// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::{parse_head_tri, Bound, Material, Mesh, Transform};

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tri() -> Vec<u8> {
    let mut out = b"FRTRI003".to_vec();
    for value in [3, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0] {
        u32le(&mut out, value);
    }
    for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for component in point {
            out.extend_from_slice(&component.to_le_bytes());
        }
    }
    for index in [0, 1, 2] {
        u32le(&mut out, index);
    }
    for _ in 0..3 {
        u32le(&mut out, 0);
    }
    u32le(&mut out, 4);
    out.extend_from_slice(b"lift");
    out.extend_from_slice(&0.01f32.to_le_bytes());
    for delta in [[0, 0, -100], [0, 0, -100], [0, 0, -100]] {
        for component in delta {
            i16le(&mut out, component);
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    out
}

fn mesh() -> Mesh {
    Mesh {
        name: "Head".into(),
        block: 0,
        transform: Transform::default(),
        bound: Bound::default(),
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        tangents: vec![[1.0, 0.0, 0.0, 1.0]; 3],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
        colors: Vec::new(),
        indices: vec![0, 1, 2],
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        skin: None,
        material: Material::default(),
    }
}

#[test]
fn edt_22_signed_morph_deltas_and_repeated_builds_use_the_declared_base_mesh() {
    let tri = parse_head_tri(&tri()).unwrap();
    let source = mesh();
    let first = tri.apply_to_mesh(&source, &[("lift", 1.0)]).unwrap();
    let second = tri.apply_to_mesh(&source, &[("lift", 1.0)]).unwrap();

    assert_eq!(first.positions, second.positions);
    assert_eq!(first.positions[0], [0.0, 0.0, -1.0]);
    assert_eq!(first.positions[1], [1.0, 0.0, -1.0]);
    assert_eq!(first.indices, source.indices);
    assert_eq!(source.positions[0], [0.0, 0.0, 0.0]);
}
