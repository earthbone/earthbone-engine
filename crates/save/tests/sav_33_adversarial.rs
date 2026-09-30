// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for xEdit's ARMO, WEAP and AMMO saved change fields.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use soe_save::{ChangeForm, RefId, Save};

const ITEM_TYPES: [(u8, &str); 3] = [(12, "ARMO"), (23, "WEAP"), (24, "AMMO")];
const CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn form(kind: u8, flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len,
        data,
    }
}

#[test]
fn sav_33_each_source_flag_combination_keeps_exact_fields_and_bytes() {
    for (kind, name) in ITEM_TYPES {
        for bits in 0..8_u32 {
            let form_section = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
            let mut payload = Vec::new();
            if bits & 1 != 0 {
                payload.extend_from_slice(&form_section);
            }
            // xEdit defines the value and full-name unions as zero-byte fields.
            payload.extend_from_slice(&[0xE7, 0xF8]);
            let change = form(kind, bits, payload.clone(), 0);
            let decoded = change.item_change().unwrap().unwrap();

            assert_eq!(decoded.record_type, name);
            assert_eq!(
                decoded.form_flags,
                (bits & 1 != 0).then_some((0xB4A3_9281, 0xD6C5))
            );
            assert_eq!(decoded.value_changed, bits & 2 != 0);
            assert_eq!(decoded.full_name_changed, bits & 4 != 0);
            assert_eq!(decoded.change_flags(), bits);
            assert_eq!(decoded.data_bytes(), payload);
        }
    }
}

#[test]
fn sav_33_every_unnamed_flag_and_zero_byte_marker_preserves_tail() {
    for (kind, _) in ITEM_TYPES {
        for bit in 3..32 {
            let flags = (1_u32 << bit) | 0b110;
            let tail = [0x00, 0xFF, 0x31];
            let decoded = form(kind, flags, tail.to_vec(), 0)
                .item_change()
                .unwrap()
                .unwrap();
            assert_eq!(decoded.unnamed_flags, 1_u32 << bit);
            assert!(decoded.value_changed);
            assert!(decoded.full_name_changed);
            assert_eq!(decoded.data_bytes(), tail);
            assert_eq!(decoded.change_flags(), flags);
        }
    }
}

#[test]
fn sav_33_compressed_payloads_decode_repeatedly_and_bad_streams_reject() {
    let original = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6, 0xE7, 0xF8];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&original).unwrap();
    let compressed = encoder.finish().unwrap();

    for (kind, _) in ITEM_TYPES {
        let change = form(kind, 0b111, compressed.clone(), original.len() as u32);
        for _ in 0..2 {
            let decoded = change.item_change().unwrap().unwrap();
            assert_eq!(decoded.form_flags, Some((0xB4A3_9281, 0xD6C5)));
            assert_eq!(decoded.value_changed, true);
            assert_eq!(decoded.full_name_changed, true);
            assert_eq!(decoded.data_bytes(), original);
        }
        assert!(form(kind, 1, vec![0x78, 0x9C, 0xFF], 8)
            .item_change()
            .is_err());
        assert!(form(kind, 1, compressed.clone(), 7).item_change().is_err());
        assert!(form(kind, 1, compressed.clone(), 9).item_change().is_err());
    }
}

#[test]
fn sav_33_truncation_is_rejected_for_each_item_type_at_every_boundary() {
    for (kind, _) in ITEM_TYPES {
        for length in 0..6 {
            let result = form(kind, 1, vec![0xA5; length], 0).item_change();
            assert!(
                matches!(result, Err(soe_save::Error::Truncated { .. })),
                "kind={kind} length={length}"
            );
        }
    }
}

#[test]
fn sav_33_adjacent_types_do_not_enter_the_item_decoder() {
    for kind in [11, 13, 22, 25] {
        assert!(form(kind, u32::MAX, vec![0xFF], 0)
            .item_change()
            .unwrap()
            .is_none());
    }
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_33_external_corpus_decodes_and_reencodes_every_item_payload() {
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    if !root.is_dir() {
        eprintln!("external save corpus unavailable, skipping SAV-33 corpus check");
        return;
    }
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");
    let mut counts = [0_usize; 3];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for changed in &save.change_forms {
            let Some(index) = ITEM_TYPES
                .iter()
                .position(|(_, name)| changed.record_type() == *name)
            else {
                continue;
            };
            let payload = changed.payload().unwrap();
            let decoded = changed.item_change().unwrap().unwrap();
            assert_eq!(decoded.data_bytes(), payload.as_ref(), "{}", path.display());
            assert_eq!(
                decoded.change_flags(),
                changed.change_flags,
                "{}",
                path.display()
            );
            counts[index] += 1;
        }
    }
    eprintln!(
        "SAV-33 external saves: ARMO={} WEAP={} AMMO={}",
        counts[0], counts[1], counts[2]
    );
}
