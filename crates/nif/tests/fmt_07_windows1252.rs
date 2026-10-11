// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! FMT-07: texture paths decode as Windows-1252, not Latin-1. The two
//! encodings agree everywhere except the 0x80 to 0x9F byte range, so every
//! test here plants a byte in that range and checks the Windows-1252
//! reading comes out, not the Latin-1 control-character reading.

mod common;

use common::{quad, Buf, NifWriter, IDENTITY};
use std::path::{Path, PathBuf};

/// `soe_nif::stream::windows1252` is the function the done check names.
/// Byte 0x88 is defined in Windows-1252 as U+02C6 (MODIFIER LETTER
/// CIRCUMFLEX ACCENT, `ˆ`). Latin-1 has no such mapping: it decodes 0x88
/// straight through to the C1 control point U+0088. The two disagree, so
/// this byte alone tells the encodings apart.
#[test]
fn fmt_07_windows1252_decodes_0x88_as_circumflex_not_the_c1_control() {
    let decoded = soe_nif::stream::windows1252(&[0x88]);
    assert_eq!(
        decoded, "\u{02C6}",
        "windows1252(0x88) should be U+02C6, not Latin-1's U+0088"
    );
    assert_ne!(decoded, "\u{0088}");
}

/// Sweeps every byte in 0x80 to 0x9F against the Windows-1252 table by
/// hand (the bytes Microsoft leaves undefined, 0x81, 0x8D, 0x8F, 0x90 and
/// 0x9D, fall back to the C1 control point under WHATWG's windows-1252
/// decode algorithm, matching Latin-1 there. Every other byte in the
/// range must differ from its Latin-1 reading).
#[test]
fn fmt_07_windows1252_table_matches_cp1252_for_defined_0x80_to_0x9f_bytes() {
    let undefined = [0x81u8, 0x8D, 0x8F, 0x90, 0x9D];
    let expected: [(u8, char); 27] = [
        (0x80, '\u{20AC}'),
        (0x82, '\u{201A}'),
        (0x83, '\u{0192}'),
        (0x84, '\u{201E}'),
        (0x85, '\u{2026}'),
        (0x86, '\u{2020}'),
        (0x87, '\u{2021}'),
        (0x88, '\u{02C6}'),
        (0x89, '\u{2030}'),
        (0x8A, '\u{0160}'),
        (0x8B, '\u{2039}'),
        (0x8C, '\u{0152}'),
        (0x8E, '\u{017D}'),
        (0x91, '\u{2018}'),
        (0x92, '\u{2019}'),
        (0x93, '\u{201C}'),
        (0x94, '\u{201D}'),
        (0x95, '\u{2022}'),
        (0x96, '\u{2013}'),
        (0x97, '\u{2014}'),
        (0x98, '\u{02DC}'),
        (0x99, '\u{2122}'),
        (0x9A, '\u{0161}'),
        (0x9B, '\u{203A}'),
        (0x9C, '\u{0153}'),
        (0x9E, '\u{017E}'),
        (0x9F, '\u{0178}'),
    ];
    for (byte, want) in expected {
        let got = soe_nif::stream::windows1252(&[byte]);
        assert_eq!(
            got.chars().next(),
            Some(want),
            "byte 0x{byte:02X} should decode to {want:?} under Windows-1252"
        );
        let latin1_reading = byte as char;
        assert_ne!(
            got.chars().next(),
            Some(latin1_reading),
            "byte 0x{byte:02X} should not fall back to its Latin-1 reading"
        );
    }
    for byte in undefined {
        let got = soe_nif::stream::windows1252(&[byte]);
        assert_eq!(got.chars().next(), Some(byte as char));
    }
}

/// A one-block scene with a texture path carrying a raw 0x88 byte, parsed
/// through the public `parse` and `Mesh` API end to end. Uses raw bytes
/// through `Buf` instead of `NifWriter::texture_set`, whose helper takes
/// `&str` and cannot carry a byte that is not valid UTF-8 on its own.
fn scene_with_high_byte_texture_path() -> Vec<u8> {
    let mut w = NifWriter::new(100);
    let raw_path: &[u8] = b"textures\\clutter\\bucket\x88.dds";
    let mut tex = Buf::default();
    tex.u32(1);
    tex.u32(raw_path.len() as u32).bytes(raw_path);
    let textures = w.add("BSShaderTextureSet", tex.0);
    let shader = w.lighting_shader(1, 0, 0, textures, &[0.5]);
    let shader = w.add("BSLightingShaderProperty", shader);
    let (verts, tris) = quad();
    let attrs = soe_nif::vertex::attr::VERTEX
        | soe_nif::vertex::attr::UVS
        | soe_nif::vertex::attr::NORMALS
        | soe_nif::vertex::attr::TANGENTS;
    let shape = w.bs_tri_shape(
        "Shape", [0.0; 3], -1, shader, -1, attrs, &verts, &tris, true,
    );
    let shape = w.add("BSTriShape", shape);
    let root = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    let root = w.add("NiNode", root);
    w.roots = vec![root];
    w.finish()
}

#[test]
fn fmt_07_texture_path_with_0x80_9f_byte_round_trips_through_windows1252() {
    let bytes = scene_with_high_byte_texture_path();
    let file = soe_nif::parse(&bytes).expect("the synthetic scene parses");
    let meshes = file.meshes().expect("the shape yields one mesh");
    assert_eq!(meshes.len(), 1);
    let diffuse = meshes[0]
        .material
        .diffuse()
        .expect("a diffuse texture path");
    assert_eq!(diffuse, "textures/clutter/bucket\u{02C6}.dds");
    assert!(
        !diffuse.contains('\u{0088}'),
        "diffuse path decoded as Latin-1 instead of Windows-1252: {diffuse:?}"
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nif"))
        {
            out.push(path);
        }
    }
}

/// SKYRIM_DATA gated: counts how many real texture paths, across every
/// loose mesh under the installed game's `meshes` folder, carry a byte in
/// the 0x80 to 0x9F range, and confirms every one of those decodes without
/// the Rust replacement character (which Windows-1252 as implemented by
/// `encoding_rs::WINDOWS_1252` never emits, since every byte has a
/// defined mapping). Skips cleanly when SKYRIM_DATA is unset, since
/// Skyrim SE ships almost all meshes inside BSA archives and this test
/// only reaches loose files.
#[test]
fn fmt_07_skyrim_data_sweep_counts_high_byte_texture_paths() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping the real-data sweep");
        return;
    };
    let root = PathBuf::from(data).join("meshes");
    let mut files = Vec::new();
    collect(&root, &mut files);
    if files.is_empty() {
        eprintln!("no .nif files under {root:?}, skipping");
        return;
    }
    let mut parsed = 0usize;
    let mut paths_seen = 0usize;
    let mut high_byte_paths = 0usize;
    for path in &files {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(file) = soe_nif::parse(&bytes) else {
            continue;
        };
        parsed += 1;
        let Ok(meshes) = file.meshes() else {
            continue;
        };
        for mesh in &meshes {
            for tex in [mesh.material.diffuse(), mesh.material.normal()] {
                let Some(tex) = tex else { continue };
                paths_seen += 1;
                assert!(
                    !tex.contains('\u{FFFD}'),
                    "{path:?} produced a Unicode replacement character in a texture path: {tex:?}"
                );
                if tex.chars().any(|c| (0x80..=0x9F).contains(&(c as u32))) {
                    high_byte_paths += 1;
                    eprintln!("{path:?}: high-byte texture path {tex:?}");
                }
            }
        }
    }
    eprintln!(
        "fmt_07 sweep: {parsed}/{} files parsed, {paths_seen} texture paths read, \
         {high_byte_paths} carried a 0x80-0x9F byte",
        files.len()
    );
}
