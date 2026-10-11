// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! FMT-07 breaker coverage, second pass. `fmt_07_windows1252.rs` and
//! `windows1252_adversarial.rs` already sweep the full byte table, the
//! Latin-1 disagreement, the BOM trap and single-value boundaries. This
//! file targets what those leave open: the divergence between
//! `sized_string` (never strips anything) and `export_string` (strips one
//! trailing NUL), zero-length edge cases on both, a dense run of every
//! 0x80 to 0x9F byte back to back in one path, two texture slots decoded
//! independently in the same block, and the `line()` reader's boundary
//! when a high byte sits at the very last position before the max cutoff
//! with no newline to find.
//!
//! Breaker only. Writes no non-test source.

mod common;

use common::{quad, Buf, NifWriter, IDENTITY};
use soe_nif::stream::Stream;
use soe_nif::vertex::attr;
use soe_nif::{parse, Error};

/// `sized_string` is length-prefixed, not NUL-terminated. An embedded NUL
/// byte inside the range the length field names must survive into the
/// decoded string untouched, unlike `export_string`, which strips exactly
/// one trailing NUL. A decoder that reused `export_string`'s trimming
/// logic for `sized_string` would silently shorten any path with a stray
/// embedded NUL.
#[test]
fn sized_string_preserves_an_embedded_nul_that_export_string_would_strip() {
    let raw: &[u8] = b"textures\\npc\\a\0b.dds";
    let mut file = Buf::default();
    file.u32(raw.len() as u32).bytes(raw);
    let mut s = Stream::new(&file.0);
    let decoded = s.sized_string().expect("well formed sized string parses");
    assert_eq!(decoded, "textures\\npc\\a\0b.dds");
    assert_eq!(decoded.len(), raw.len(), "embedded NUL must not be dropped");
}

/// `export_string` strips exactly one trailing NUL, and only when the
/// declared length is 0 is there nothing to strip. A length of 0 must
/// decode to the empty string without underflowing the length-minus-one
/// arithmetic a naive implementation might use.
#[test]
fn export_string_with_zero_length_decodes_to_empty_without_underflow() {
    let mut file = Buf::default();
    file.u8(0);
    let mut s = Stream::new(&file.0);
    let decoded = s.export_string().expect("zero length export string parses");
    assert_eq!(decoded, "");
    assert_eq!(s.remaining(), 0);
}

/// `export_string` with length 1 holding only the terminating NUL, no
/// characters before it. Confirms the strip finds the NUL and does not
/// instead read one byte past what the length declared.
#[test]
fn export_string_with_only_the_nul_decodes_to_empty() {
    let mut file = Buf::default();
    file.u8(1).u8(0);
    let mut s = Stream::new(&file.0);
    let decoded = s.export_string().expect("nul only export string parses");
    assert_eq!(decoded, "");
}

/// A high byte that is itself 0x00 cannot occur (0x00 is plain ASCII NUL,
/// not in the 0x80 to 0x9F range), but a high byte immediately followed
/// by a real NUL inside an `export_string` payload must still decode the
/// high byte correctly and strip only the trailing NUL, not the high
/// byte.
#[test]
fn export_string_strips_only_the_trailing_nul_not_a_preceding_high_byte() {
    let mut raw = vec![0x8Au8]; // LATIN CAPITAL LETTER S WITH CARON
    raw.push(0);
    let mut file = Buf::default();
    file.u8(raw.len() as u8).bytes(&raw);
    let mut s = Stream::new(&file.0);
    let decoded = s.export_string().expect("high byte plus nul parses");
    assert_eq!(decoded, "\u{0160}");
}

/// Every byte from 0x80 to 0x9F back to back in a single path, so any bug
/// that only shows up when the decoder's internal state carries across
/// consecutive high bytes (a stubbed loop that only handles an isolated
/// high byte surrounded by ASCII) has 32 bytes in a row to fail on.
#[test]
fn a_dense_run_of_every_high_byte_decodes_correctly_in_one_path() {
    let dense: Vec<u8> = (0x80u8..=0x9F).collect();
    let mut raw = b"textures\\dense\\".to_vec();
    raw.extend_from_slice(&dense);
    raw.extend_from_slice(b".dds");

    let mut file = Buf::default();
    file.u32(raw.len() as u32).bytes(&raw);
    let mut s = Stream::new(&file.0);
    let decoded = s.sized_string().expect("dense run parses");

    let expected: String = "textures\\dense\\".to_string()
        + &dense
            .iter()
            .map(|&b| {
                let single = [b];
                let (chunk, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&single);
                chunk.into_owned()
            })
            .collect::<String>()
        + ".dds";
    assert_eq!(decoded, expected);
    assert_eq!(
        decoded.chars().filter(|&c| c == '\u{FFFD}').count(),
        0,
        "no byte in a dense high run should fall back to the replacement character"
    );
}

/// Two texture slots in the same BSShaderTextureSet, each carrying a
/// different high byte. Confirms the decode of one slot cannot bleed into
/// or be overwritten by the decode of the other, the kind of bug a shared
/// mutable decode buffer would produce.
#[test]
fn two_texture_slots_with_different_high_bytes_decode_independently() {
    let mut diffuse_raw = b"textures\\armor\\steel\\".to_vec();
    diffuse_raw.push(0x93); // LEFT DOUBLE QUOTATION MARK
    diffuse_raw.extend_from_slice(b"d.dds");

    let mut normal_raw = b"textures\\armor\\steel\\".to_vec();
    normal_raw.push(0x94); // RIGHT DOUBLE QUOTATION MARK
    normal_raw.extend_from_slice(b"n.dds");

    let mut w = NifWriter::new(100);
    let mut tex = Buf::default();
    tex.u32(2);
    tex.u32(diffuse_raw.len() as u32).bytes(&diffuse_raw);
    tex.u32(normal_raw.len() as u32).bytes(&normal_raw);
    let textures = w.add("BSShaderTextureSet", tex.0);

    let shader = w.lighting_shader(1, 0, 0, textures, &[0.5]);
    let shader = w.add("BSLightingShaderProperty", shader);

    let (verts, tris) = quad();
    let attrs = attr::VERTEX | attr::UVS;
    let shape = w.bs_tri_shape(
        "Armor:0", [0.0; 3], -1, shader, -1, attrs, &verts, &tris, true,
    );
    let shape = w.add("BSTriShape", shape);
    let root = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[shape]);
    let root = w.add("NiNode", root);
    w.roots = vec![root];

    let bytes = w.finish();
    let file = parse(&bytes).expect("the synthetic scene parses");
    let meshes = file.meshes().expect("mesh extraction succeeds");
    assert_eq!(meshes.len(), 1);

    let diffuse = meshes[0].material.diffuse().expect("diffuse path present");
    let normal = meshes[0].material.normal().expect("normal path present");

    assert_eq!(diffuse, "textures/armor/steel/\u{201C}d.dds");
    assert_eq!(normal, "textures/armor/steel/\u{201D}n.dds");
    assert!(
        !diffuse.contains('\u{201D}'),
        "diffuse should not carry the normal slot's quote"
    );
    assert!(
        !normal.contains('\u{201C}'),
        "normal should not carry the diffuse slot's quote"
    );
}

/// `line()` scans for a terminating `\n` within `max` bytes and returns
/// `Error::BadMagic` if none is found. A high byte sitting in the very
/// last scanned position, with no newline anywhere in range, must still
/// report the error and must not decode a partial, unterminated line as
/// if it had found the end.
#[test]
fn line_with_a_high_byte_at_the_scan_limit_and_no_newline_errors() {
    let mut raw = vec![b'B', b'y'];
    raw.push(0x92); // RIGHT SINGLE QUOTATION MARK, last byte, no newline follows
    let mut s = Stream::new(&raw);
    let err = s
        .line(raw.len())
        .expect_err("a line with no newline in range must fail, not decode");
    assert!(matches!(err, Error::BadMagic), "got {err:?}, want BadMagic");
}

/// `line()` succeeds when the newline is the byte immediately after a
/// high byte that itself sits at `max - 1`, the tightest boundary where
/// the scan limit and the newline position coincide.
#[test]
fn line_with_a_high_byte_immediately_before_the_newline_at_the_limit_decodes() {
    let raw = vec![b'B', b'y', 0x92, b'\n'];
    let max = raw.len();
    let mut s = Stream::new(&raw);
    let decoded = s
        .line(max)
        .expect("newline exactly at the scan limit parses");
    assert_eq!(decoded, "By\u{2019}");
}
