// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::Archive;
use soe_nif::{parse_head_tri, Error};
use std::path::Path;

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn i16le(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn synthetic_head_tri() -> Vec<u8> {
    let mut out = b"FRTRI003".to_vec();
    for value in [4, 1, 0, 0, 0, 0, 1, 2, 1, 0, 0, 0, 0, 0] {
        u32le(&mut out, value);
    }
    for vertex in [
        [0.0f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [2.0, 2.0, 2.0],
    ] {
        for component in vertex {
            out.extend_from_slice(&component.to_le_bytes());
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 1);
    u32le(&mut out, 2);
    out.extend_from_slice(&[0; 12]);

    for (name, values) in [
        ("lift", [[0, 0, 0], [100, 0, 0], [0, 200, 0], [0, 0, 0]]),
        ("other", [[0, 0, 300], [0, 0, 0], [0, 0, 0], [0, 0, 0]]),
    ] {
        u32le(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&0.01f32.to_le_bytes());
        for [x, y, z] in values {
            i16le(&mut out, x);
            i16le(&mut out, y);
            i16le(&mut out, z);
        }
    }
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    out
}

#[test]
fn anm_10_selected_morph_changes_only_its_declared_vertices() {
    let tri = parse_head_tri(&synthetic_head_tri()).expect("valid synthetic head TRI");
    let base = tri.vertices();
    let lifted = tri
        .apply_morph(base, "lift", 1.0)
        .expect("selected morph applies");

    assert_eq!(lifted[0], base[0], "zero deltas preserve their vertices");
    assert_eq!(lifted[1], [2.0, 0.0, 0.0], "first declared delta applies");
    assert_eq!(lifted[2], [0.0, 3.0, 0.0], "second declared delta applies");
    assert_eq!(lifted[3], base[3], "undeformed vertex stays at its base");
}

#[test]
fn anm_10_zero_weight_restores_base_head() {
    let tri = parse_head_tri(&synthetic_head_tri()).expect("valid synthetic head TRI");
    let base = tri.vertices();
    let morphed = tri
        .apply_morph(base, "lift", 1.0)
        .expect("selected morph applies");
    assert_ne!(morphed, base, "nonzero morph changes the head");

    let restored = tri
        .apply_morph(base, "lift", 0.0)
        .expect("zero morph weight restores the base");
    assert_eq!(restored, base, "zero weight returns exact base vertices");
}

#[test]
fn anm_10_rejects_vertex_count_mismatch_and_unknown_morph() {
    let tri = parse_head_tri(&synthetic_head_tri()).expect("valid synthetic head TRI");
    assert!(matches!(
        tri.apply_morph(&[[0.0; 3]], "lift", 1.0),
        Err(Error::HeadMorphVertexCount {
            expected: 4,
            actual: 1
        })
    ));
    assert!(matches!(
        tri.apply_morph(tri.vertices(), "missing", 1.0),
        Err(Error::HeadMorphNotFound(_))
    ));
}

#[test]
fn anm_10_rejects_truncated_head_tri_data() {
    let bytes = synthetic_head_tri();
    assert!(matches!(
        parse_head_tri(&bytes[..bytes.len() - 30]),
        Err(Error::Truncated { .. })
    ));
}

#[test]
fn anm_10_vanilla_head_tri_morphs_match_their_base_vertices() {
    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA is unset, skipping vanilla head TRI survey");
        return;
    };
    let archive_path = Path::new(&data_dir).join("Skyrim - Meshes0.bsa");
    let archive = Archive::open(&archive_path).expect("open vanilla mesh archive");
    let mut found = false;

    for path in archive
        .files()
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".tri"))
    {
        let bytes = archive.read(path).expect("read archived TRI");
        if !bytes.starts_with(b"FRTRI") {
            continue;
        }
        let tri = parse_head_tri(&bytes).expect("parse vanilla head TRI");
        if tri.morphs().is_empty() {
            continue;
        }
        let morph = &tri.morphs()[0];
        let changed = tri
            .apply_morph(tri.vertices(), morph.name(), 1.0)
            .expect("apply vanilla head morph");
        let expected_changed = morph
            .deltas()
            .iter()
            .filter(|delta| **delta != [0.0; 3])
            .count();
        assert_eq!(
            changed
                .iter()
                .zip(tri.vertices())
                .filter(|(actual, base)| actual != base)
                .count(),
            expected_changed,
            "only vertices with a declared nonzero delta should move in {path}"
        );
        assert_eq!(
            tri.apply_morph(tri.vertices(), morph.name(), 0.0)
                .expect("zero weight restores vanilla head"),
            tri.vertices(),
            "zero weight restores the base head in {path}"
        );
        found = true;
        break;
    }

    assert!(
        found,
        "vanilla mesh archive should contain a morphable head TRI"
    );
}
