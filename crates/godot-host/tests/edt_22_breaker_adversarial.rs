// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::facegen::build_head_part_surface;
use soe_nif::{parse_head_tri, Bound, Material, Mesh, Transform};

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tri_bytes() -> Vec<u8> {
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
fn edt_22_multiple_morphs_keep_topology_and_move_geometry() {
    let tri = parse_head_tri(&tri_bytes()).unwrap();
    let source = mesh();
    let base = build_head_part_surface(&source, &tri, &[]).expect("base head renders");
    let surface = build_head_part_surface(&source, &tri, &[("lift", 1.0), ("push", 0.5)])
        .expect("declared morphs build a host surface");

    assert_eq!(surface.positions.len(), 3);
    assert_eq!(surface.triangle_count(), 1);
    assert_eq!(surface.uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    for (generated, original) in surface.positions.iter().zip(&base.positions) {
        assert!((generated[0] - original[0] - 0.5 * 0.014_287_5).abs() < 1.0e-6);
        assert!((generated[1] - original[1] - 0.014_287_5).abs() < 1.0e-6);
    }
    assert!(surface.bounds.max.y > base.bounds.max.y);
}

#[test]
fn edt_22_wrong_base_mesh_is_rejected_before_surface_generation() {
    let tri = parse_head_tri(&tri_bytes()).unwrap();
    let mut source = mesh();
    source.positions[1][0] += 0.25;

    assert!(build_head_part_surface(&source, &tri, &[("lift", 1.0)]).is_err());
}

#[test]
fn edt_22_missing_morph_and_nonfinite_weight_are_rejected() {
    let tri = parse_head_tri(&tri_bytes()).unwrap();
    let source = mesh();

    assert!(build_head_part_surface(&source, &tri, &[("absent", 1.0)]).is_err());
    assert!(build_head_part_surface(&source, &tri, &[("lift", f32::NAN)]).is_err());
}

#[test]
fn edt_22_repeated_generation_does_not_accumulate_morphs() {
    let tri = parse_head_tri(&tri_bytes()).unwrap();
    let source = mesh();
    let first = build_head_part_surface(&source, &tri, &[("lift", 1.0)]).unwrap();
    let second = build_head_part_surface(&source, &tri, &[("lift", 1.0)]).unwrap();

    assert_eq!(first.positions, second.positions);
    assert_eq!(source.positions[0], [0.0, 0.0, 0.0]);
}
