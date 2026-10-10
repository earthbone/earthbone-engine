// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_nif::parse_head_tri;
use std::path::PathBuf;

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn tri() -> Vec<u8> {
    let mut out = b"FRTRI003".to_vec();
    for value in [3, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0] {
        u32le(&mut out, value);
    }
    for vertex in [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]] {
        for component in vertex {
            out.extend_from_slice(&component.to_le_bytes());
        }
    }
    u32le(&mut out, 4);
    out.extend_from_slice(b"move");
    out.extend_from_slice(&0.5f32.to_le_bytes());
    for delta in [[2, 0, 0], [0, -4, 0], [0, 0, 0]] {
        for component in delta {
            i16le(&mut out, component);
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    out
}

#[test]
fn morph_weights_are_measured_from_the_unchanged_base_on_each_call() {
    let tri = parse_head_tri(&tri()).expect("synthetic head TRI parses");
    let base = tri.vertices();
    let full = tri.apply_morph(base, "move", 1.0).expect("full weight");
    let half = tri.apply_morph(base, "move", 0.5).expect("half weight");
    let zero = tri.apply_morph(base, "move", 0.0).expect("zero weight");

    assert_eq!(full, [[2.0, 2.0, 3.0], [4.0, 3.0, 6.0], [7.0, 8.0, 9.0]]);
    assert_eq!(half, [[1.5, 2.0, 3.0], [4.0, 4.0, 6.0], [7.0, 8.0, 9.0]]);
    assert_eq!(zero, base);
}

#[test]
fn vanilla_head_tri_morphs_move_only_declared_vertices_and_zero_restores_base() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Meshes0.bsa"))
        .expect("open vanilla mesh archive");
    let mut sampled = 0;

    for path in archive
        .files()
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".tri"))
    {
        let bytes = archive.read(path).expect("read vanilla TRI");
        if !bytes.starts_with(b"FRTRI003") {
            continue;
        }
        let tri = parse_head_tri(&bytes).expect("parse vanilla head TRI");
        let Some(morph) = tri
            .morphs()
            .iter()
            .find(|morph| morph.deltas().iter().any(|delta| *delta != [0.0; 3]))
        else {
            continue;
        };
        let base = tri.vertices();
        let moved = tri
            .apply_morph(base, morph.name(), 1.0)
            .expect("apply vanilla morph");
        for (index, (actual, original)) in moved.iter().zip(base).enumerate() {
            let has_delta = morph.deltas()[index] != [0.0; 3];
            assert_eq!(actual != original, has_delta, "vertex {index} in {path}");
        }
        assert_eq!(
            tri.apply_morph(base, morph.name(), 0.0)
                .expect("zero weight restores head"),
            base,
            "zero weight restores base vertices in {path}"
        );
        sampled += 1;
        if sampled == 3 {
            break;
        }
    }

    assert_eq!(
        sampled, 3,
        "three vanilla morphable head TRI files are sampled"
    );
}
