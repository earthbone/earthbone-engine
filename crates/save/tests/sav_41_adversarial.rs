// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{io::Write, path::Path};

use soe_save::base_object::static_object_flags as flags;
use soe_save::{ChangeForm, RefId, Save};

fn change(kind: u8, change_flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x01, 0x23]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len,
        data,
    }
}

#[test]
fn sav_41_high_flags_and_zero_byte_markers_do_not_consume_payload() {
    for kind in [14, 18, 22] {
        let suffix = [0, 1, 0xFE, 0xFF];
        let flags_word = flags::VALUE | flags::FULL_NAME | (1 << 31) | (1 << 3);
        let decoded = change(kind, flags_word, suffix.to_vec(), 0)
            .static_object_change()
            .unwrap()
            .unwrap();

        assert!(decoded.form_flags.is_none());
        assert!(decoded.value_changed);
        assert!(decoded.full_name_changed);
        assert_eq!(decoded.unnamed_flags, (1 << 31) | (1 << 3));
        assert_eq!(decoded.data_bytes().unwrap(), suffix);
        assert_eq!(decoded.change_flags().unwrap(), flags_word);
    }
}

#[test]
fn sav_41_form_prefix_keeps_suffix_and_checks_exact_boundary() {
    let data = [0x78, 0x56, 0x34, 0x12, 0xBC, 0x9A, 0x00, 0xFF];
    for kind in [14, 18, 22] {
        let decoded = change(kind, flags::FORM | (1 << 29), data.to_vec(), 0)
            .static_object_change()
            .unwrap()
            .unwrap();

        assert_eq!(decoded.form_flags, Some((0x1234_5678, 0x9ABC)));
        assert_eq!(decoded.opaque_suffix, [0x00, 0xFF]);
        assert_eq!(decoded.data_bytes().unwrap(), data);

        for cut in 0..6 {
            assert!(change(kind, flags::FORM, vec![0xA5; cut], 0)
                .static_object_change()
                .is_err());
        }
    }
}

#[test]
fn sav_41_decompresses_before_decoding_and_rejects_bad_compressed_size() {
    let raw = [0xEF, 0xBE, 0xAD, 0xDE, 0x10, 0x20, 0x30, 0x40];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    let compressed = encoder.finish().unwrap();

    for kind in [14, 18, 22] {
        let decoded = change(
            kind,
            flags::FORM | flags::VALUE,
            compressed.clone(),
            raw.len() as u32,
        )
        .static_object_change()
        .unwrap()
        .unwrap();
        assert_eq!(decoded.form_flags, Some((0xDEAD_BEEF, 0x2010)));
        assert_eq!(decoded.opaque_suffix, [0x30, 0x40]);
        assert_eq!(decoded.data_bytes().unwrap(), raw);

        assert!(
            change(kind, flags::FORM, compressed.clone(), raw.len() as u32 + 1)
                .static_object_change()
                .is_err()
        );
    }
}

#[test]
fn sav_41_changed_known_flag_without_matching_field_is_rejected() {
    let mut decoded = change(14, flags::FORM, vec![1, 2, 3, 4, 5, 6], 0)
        .static_object_change()
        .unwrap()
        .unwrap();
    decoded.change_flags &= !flags::FORM;

    assert!(decoded.change_flags().is_err());
    assert!(decoded.data_bytes().is_err());
}

#[test]
fn sav_41_only_cont_misc_and_furn_use_static_object_layout() {
    for kind in [0, 1, 13, 17, 23, 48] {
        assert!(change(kind, u32::MAX, vec![0xAA], 0)
            .static_object_change()
            .unwrap()
            .is_none());
    }
}

fn collect_saves(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_41_external_save_corpus_accounts_for_target_payloads() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping external save survey");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = [0usize; 3];
    let mut bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Some(index) = ["CONT", "FURN", "MISC"]
                .iter()
                .position(|record_type| *record_type == change.record_type())
            else {
                continue;
            };
            counts[index] += 1;
            let raw = change.payload().unwrap();
            let decoded = change.static_object_change().unwrap().unwrap();
            assert_eq!(decoded.data_bytes().unwrap(), raw.as_ref());
            bytes += raw.len();
        }
    }

    assert_eq!(counts, [0, 0, 0]);
    assert_eq!(bytes, 0);
    eprintln!(
        "SAV-41 surveyed {} saves, counts CONT/FURN/MISC={counts:?}, payload bytes={bytes}",
        paths.len()
    );
}
