// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::{parse_head_tri, Bound, Material, Mesh, Transform};

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn head_tri() -> Vec<u8> {
    let mut out = b"FRTRI003".to_vec();
    for value in [3, 1, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0] {
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
    for (name, delta) in [("lift", [0, 0, 100]), ("push", [100, 0, 0])] {
        u32le(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&0.01f32.to_le_bytes());
        for _ in 0..3 {
            for component in delta {
                i16le(&mut out, component);
            }
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    out
}

fn head_part() -> Mesh {
    Mesh {
        name: "HeadPart".into(),
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
fn edt_22_declared_morphs_generate_matching_head_part_geometry() {
    let tri = parse_head_tri(&head_tri()).expect("valid head TRI");
    let source = head_part();
    let generated = tri
        .apply_to_mesh(&source, &[("lift", 1.0), ("push", 0.5)])
        .expect("morphs apply to the declared head-part mesh");

    assert_eq!(
        generated.positions,
        [[0.5, 0.0, 1.0], [1.5, 0.0, 1.0], [0.5, 1.0, 1.0]]
    );
    assert_eq!(generated.indices, source.indices);
    assert_eq!(generated.uvs, source.uvs);
    assert_eq!(generated.material, source.material);
    assert!(generated.normals.is_empty());
    assert!(generated.tangents.is_empty());
    assert_eq!(
        source.positions,
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
}

#[test]
fn edt_22_rejects_morphs_that_do_not_match_the_declared_mesh() {
    let tri = parse_head_tri(&head_tri()).expect("valid head TRI");
    let mut source = head_part();
    source.positions.pop();

    assert!(matches!(
        tri.apply_to_mesh(&source, &[("lift", 1.0)]),
        Err(soe_nif::Error::HeadMorphVertexCount {
            expected: 3,
            actual: 2
        })
    ));
}

#[test]
fn edt_22_rejects_same_sized_mesh_with_different_base_geometry() {
    let tri = parse_head_tri(&head_tri()).expect("valid head TRI");
    let mut source = head_part();
    source.positions[1][0] = 1.25;

    assert!(matches!(
        tri.apply_to_mesh(&source, &[("lift", 1.0)]),
        Err(soe_nif::Error::HeadMorphBaseMismatch { vertex: 1 })
    ));
}
