// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for FMT-07 (decode texture paths as Windows-1252,
//! not Latin-1). Targets `stream::windows1252` directly, the three callers
//! that route through it (`sized_string`, `export_string`, `line`), and one
//! full asset-path round trip through a real BSShaderTextureSet block.
//!
//! Breaker only. Writes no non-test source.

mod common;

use common::{quad, Buf, NifWriter, IDENTITY};
use soe_nif::blocks::{flags1, flags2};
use soe_nif::stream::{windows1252, Stream};
use soe_nif::vertex::attr;
use soe_nif::{parse, Error};

/// The WHATWG / Windows-1252 mapping for the C1 control byte range
/// (0x80 to 0x9F), the exact range the done check names. Five code points
/// (0x81, 0x8D, 0x8F, 0x90, 0x9D) are unassigned in the standard and map to
/// their own byte value as a control character, identical to Latin-1 there.
/// Every other byte in the range differs from Latin-1, where the same byte
/// would map straight to U+0080..U+009F.
const WINDOWS_1252_HIGH_RANGE: [(u8, char); 32] = [
    (0x80, '\u{20AC}'), // EURO SIGN
    (0x81, '\u{0081}'), // unassigned, falls back to the control point
    (0x82, '\u{201A}'), // SINGLE LOW-9 QUOTATION MARK
    (0x83, '\u{0192}'), // LATIN SMALL LETTER F WITH HOOK
    (0x84, '\u{201E}'), // DOUBLE LOW-9 QUOTATION MARK
    (0x85, '\u{2026}'), // HORIZONTAL ELLIPSIS
    (0x86, '\u{2020}'), // DAGGER
    (0x87, '\u{2021}'), // DOUBLE DAGGER
    (0x88, '\u{02C6}'), // MODIFIER LETTER CIRCUMFLEX ACCENT
    (0x89, '\u{2030}'), // PER MILLE SIGN
    (0x8A, '\u{0160}'), // LATIN CAPITAL LETTER S WITH CARON
    (0x8B, '\u{2039}'), // SINGLE LEFT-POINTING ANGLE QUOTATION MARK
    (0x8C, '\u{0152}'), // LATIN CAPITAL LIGATURE OE
    (0x8D, '\u{008D}'), // unassigned
    (0x8E, '\u{017D}'), // LATIN CAPITAL LETTER Z WITH CARON
    (0x8F, '\u{008F}'), // unassigned
    (0x90, '\u{0090}'), // unassigned
    (0x91, '\u{2018}'), // LEFT SINGLE QUOTATION MARK
    (0x92, '\u{2019}'), // RIGHT SINGLE QUOTATION MARK
    (0x93, '\u{201C}'), // LEFT DOUBLE QUOTATION MARK
    (0x94, '\u{201D}'), // RIGHT DOUBLE QUOTATION MARK
    (0x95, '\u{2022}'), // BULLET
    (0x96, '\u{2013}'), // EN DASH
    (0x97, '\u{2014}'), // EM DASH
    (0x98, '\u{02DC}'), // SMALL TILDE
    (0x99, '\u{2122}'), // TRADE MARK SIGN
    (0x9A, '\u{0161}'), // LATIN SMALL LETTER S WITH CARON
    (0x9B, '\u{203A}'), // SINGLE RIGHT-POINTING ANGLE QUOTATION MARK
    (0x9C, '\u{0153}'), // LATIN SMALL LIGATURE OE
    (0x9D, '\u{009D}'), // unassigned
    (0x9E, '\u{017E}'), // LATIN SMALL LETTER Z WITH CARON
    (0x9F, '\u{0178}'), // LATIN CAPITAL LETTER Y WITH DIAERESIS
];

#[test]
fn every_byte_in_0x80_0x9f_decodes_to_its_windows_1252_character() {
    for &(byte, want) in &WINDOWS_1252_HIGH_RANGE {
        let got = windows1252(&[byte]);
        assert_eq!(
            got.chars().next(),
            Some(want),
            "byte 0x{byte:02X} decoded to {got:?}, want {want:?} (U+{:04X})",
            want as u32
        );
    }
}

#[test]
fn assigned_high_bytes_disagree_with_the_naive_latin1_reading() {
    // Latin-1 maps every byte in this range straight to the same code
    // point (U+0080..U+009F). If `windows1252` regressed to Latin-1, this
    // test would start passing on the assigned bytes and the bug would go
    // unnoticed. 27 of the 32 bytes must differ, only the five unassigned
    // ones legitimately agree.
    let mut disagreements = 0;
    for &(byte, want) in &WINDOWS_1252_HIGH_RANGE {
        let latin1_reading = byte as u32;
        if want as u32 != latin1_reading {
            disagreements += 1;
        }
    }
    assert_eq!(
        disagreements, 27,
        "expected 27 of 32 high bytes to differ from their Latin-1 reading"
    );
}

#[test]
fn a_texture_path_with_high_bytes_round_trips_through_sized_string() {
    // "texturesE28. dds" is not the intent, this is a hostile path: the
    // directory separator byte is followed by 0x93 0x94 (curly quotes in
    // Windows-1252), the kind of byte Bethesda authoring tools do emit in
    // path comments and that a Latin-1 reader would render as C1 control
    // characters instead of quotation marks.
    let mut raw = Vec::new();
    raw.extend_from_slice(b"textures\\npc\\");
    raw.push(0x93); // should decode to LEFT DOUBLE QUOTATION MARK
    raw.extend_from_slice(b"unique");
    raw.push(0x94); // should decode to RIGHT DOUBLE QUOTATION MARK
    raw.extend_from_slice(b".dds");

    let mut file = Buf::default();
    file.u32(raw.len() as u32).bytes(&raw);

    let mut s = Stream::new(&file.0);
    let decoded = s.sized_string().expect("well formed sized string parses");

    assert!(
        decoded.contains('\u{201C}'),
        "missing left double quote: {decoded:?}"
    );
    assert!(
        decoded.contains('\u{201D}'),
        "missing right double quote: {decoded:?}"
    );
    assert!(
        !decoded.contains('\u{0093}') && !decoded.contains('\u{0094}'),
        "path still carries C1 control characters, Latin-1 reading leaked through: {decoded:?}"
    );
    assert_eq!(decoded, "textures\\npc\\\u{201C}unique\u{201D}.dds");
}

#[test]
fn export_string_decodes_high_bytes_the_same_way() {
    // ExportString: u8 length including the trailing NUL.
    let mut raw = vec![0x99]; // TRADE MARK SIGN in Windows-1252
    raw.extend_from_slice(b"Steel");
    let mut file = Buf::default();
    file.u8((raw.len() + 1) as u8).bytes(&raw).u8(0);

    let mut s = Stream::new(&file.0);
    let decoded = s.export_string().expect("well formed export string parses");
    assert_eq!(decoded, "\u{2122}Steel");
}

#[test]
fn header_line_decodes_high_bytes_the_same_way() {
    let mut raw = vec![b'B', b'y', 0x92]; // RIGHT SINGLE QUOTATION MARK
    raw.push(b'\n');
    let mut s = Stream::new(&raw);
    let decoded = s.line(raw.len()).expect("newline terminated line parses");
    assert_eq!(decoded, "By\u{2019}");
}

#[test]
fn every_single_byte_value_decodes_without_panicking_or_substituting() {
    // Windows-1252 is a total, bijective mapping over 0..=255 except for
    // the five unassigned C1 slots, which still map to a real code point
    // rather than the replacement character. A single malformed byte must
    // never produce U+FFFD and must never panic, unlike a UTF-8 decode.
    for byte in 0u8..=255 {
        let decoded = windows1252(&[byte]);
        assert_eq!(
            decoded.chars().count(),
            1,
            "byte 0x{byte:02X} produced {decoded:?}"
        );
        assert_ne!(
            decoded.chars().next(),
            Some('\u{FFFD}'),
            "byte 0x{byte:02X} decoded to the replacement character"
        );
    }
}

#[test]
fn a_truncated_sized_string_in_the_high_range_still_reports_an_error_not_garbage() {
    // The length field claims 4 bytes but only 2 follow. The bounds check
    // in `count` must fire before any Windows-1252 decoding happens, high
    // bytes or not. `count` reports the oversized length as `CountTooLarge`
    // rather than `Truncated`, since the length field itself is the thing
    // that does not fit, not a later read.
    let mut file = Buf::default();
    file.u32(4).bytes(&[0x93, 0x94]);
    let mut s = Stream::new(&file.0);
    let err = s
        .sized_string()
        .expect_err("truncated string must not silently decode");
    assert!(
        matches!(err, Error::CountTooLarge { .. }),
        "got {err:?}, want CountTooLarge"
    );
}

#[test]
fn a_texture_set_with_high_byte_paths_decodes_correctly_through_a_real_mesh() {
    // End to end: a BSShaderTextureSet block inside a full NIF, read
    // through blocks.rs's texture set parser and exposed on Material.
    // This is the actual path the done check names: a texture path using
    // the 0x80 to 0x9F range must resolve to the correct characters when
    // a real mesh is parsed, not just when the decoder is unit tested.
    let mut diffuse_raw = b"textures\\armor\\steel\\".to_vec();
    diffuse_raw.push(0x9A); // LATIN SMALL LETTER S WITH CARON
    diffuse_raw.extend_from_slice(b"teel_d.dds");

    let mut w = NifWriter::new(100);

    // Build the texture set manually with raw bytes, `NifWriter::texture_set`
    // only accepts `&str` and these bytes are not valid UTF-8 on their own.
    let mut tex = Buf::default();
    tex.u32(2);
    tex.u32(diffuse_raw.len() as u32).bytes(&diffuse_raw);
    tex.sized_string("textures\\armor\\steel\\steel_n.dds");
    let textures = w.add("BSShaderTextureSet", tex.0);

    let shader = w.lighting_shader(1, flags1::SPECULAR, flags2::ZBUFFER_WRITE, textures, &[0.5]);
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

    let diffuse = meshes[0].material.diffuse().expect("diffuse texture set");
    assert!(
        diffuse.contains('\u{0161}'),
        "expected LATIN SMALL LETTER S WITH CARON in {diffuse:?}"
    );
    assert!(
        !diffuse.contains('\u{009A}'),
        "path still carries the Latin-1 C1 control reading: {diffuse:?}"
    );
    assert_eq!(diffuse, "textures/armor/steel/\u{0161}teel_d.dds");
}

/// `decode_without_bom_handling` must not sniff or strip a leading byte
/// sequence that happens to look like a UTF-8 BOM (0xEF 0xBB 0xBF). Windows-1252
/// has no BOM concept, so those three bytes must decode as three ordinary
/// Windows-1252 characters (ï, right guillemet, ¿). Calling `decode_bom_handling` instead
/// (a one-word method-name mistake `encoding_rs` makes easy to introduce)
/// would silently eat those bytes from a real path that happens to start
/// with them.
#[test]
fn a_path_starting_with_utf8_bom_bytes_is_not_treated_as_a_bom() {
    let raw = [0xEFu8, 0xBB, 0xBF, b't', b'x'];
    let decoded = windows1252(&raw);
    assert_eq!(decoded.chars().count(), 5, "got {decoded:?}");
    assert_eq!(decoded, "\u{00EF}\u{00BB}\u{00BF}tx");
}

/// Every byte value 0x00 to 0xFF, round tripped through the real
/// `Stream::sized_string` entry point rather than calling `windows1252`
/// directly, one byte per string, back to back in a single buffer. Catches
/// a bug that only shows up through `take()` and `count()` bounds
/// handling (an off-by-one at the exact end of the buffer, a stray byte
/// eaten between reads) that a direct `windows1252(&[byte])` call would
/// never exercise.
#[test]
fn every_byte_value_round_trips_through_sized_string_back_to_back() {
    let mut file = Buf::default();
    for byte in 0u8..=255 {
        file.u32(1).bytes(&[byte]);
    }
    let mut s = Stream::new(&file.0);
    for byte in 0u8..=255 {
        let decoded = s
            .sized_string()
            .unwrap_or_else(|e| panic!("byte 0x{byte:02X}: {e:?}"));
        let want = windows1252(&[byte]);
        assert_eq!(
            decoded, want,
            "byte 0x{byte:02X}: back-to-back sized_string read {decoded:?}, want {want:?}"
        );
    }
    assert_eq!(s.remaining(), 0, "buffer should be exactly consumed");
}

/// A high byte sitting in the very last position of the buffer, with the
/// sized-string length claiming exactly the bytes available, no more and
/// no less. Exercises the boundary where `take()` computes
/// `self.pos + n == self.data.len()` exactly.
#[test]
fn a_high_byte_at_the_exact_end_of_the_buffer_decodes_correctly() {
    let mut file = Buf::default();
    file.u32(1).bytes(&[0x9F]); // LATIN CAPITAL LETTER Y WITH DIAERESIS
    let mut s = Stream::new(&file.0);
    assert_eq!(s.remaining(), file.0.len());
    let decoded = s.sized_string().expect("exact-length string parses");
    assert_eq!(decoded, "\u{0178}");
    assert_eq!(s.remaining(), 0);
}
