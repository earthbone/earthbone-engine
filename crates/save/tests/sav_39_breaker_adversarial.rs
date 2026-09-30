// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-39 checks for unnamed source flags and corpus coverage.

use std::{io::Write, path::{Path, PathBuf}};

use soe_save::quest::FormFlags;
use soe_save::{ChangeForm, RefId, Save};

fn change(kind: u8, flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
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
fn sav_39_all_defined_flag_combinations_decode_from_compressed_payloads() {
    for kind in [17, 20, 21] {
        for flags in 0..=7 {
            let expected = if flags & 1 == 0 {
                Vec::new()
            } else {
                vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
            };
            let (encoded, uncompressed_len) = if expected.is_empty() {
                (expected.clone(), 0)
            } else {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(&expected).unwrap();
                (encoder.finish().unwrap(), expected.len() as u32)
            };
            let decoded = change(kind, flags, encoded, uncompressed_len)
                .light_static_change()
                .unwrap()
                .unwrap();
            assert_eq!(decoded.change_flags(), flags);
            assert_eq!(decoded.data_bytes(), expected);
            assert_eq!(
                decoded.form_flags,
                (flags & 1 != 0).then_some(FormFlags {
                    flags: 0xB4A3_9281,
                    unknown: 0xD6C5,
                })
            );
            assert_eq!(decoded.value_changed, flags & 2 != 0);
            assert_eq!(decoded.full_name_changed, flags & 4 != 0);
        }
    }
}

fn collect_saves(path: &Path, paths: &mut Vec<PathBuf>) {
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
fn sav_39_corpus_audit_requires_and_accounts_for_the_external_save_set() {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || PathBuf::from("/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves"),
        PathBuf::from,
    );
    let mut paths = Vec::new();
    collect_saves(&root, &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = [0usize; 3];
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(index) = ["LIGH", "STAT", "MSTT"]
                .iter()
                .position(|record_type| form.record_type() == *record_type)
            else {
                continue;
            };
            counts[index] += 1;
            let payload = form.payload().unwrap();
            payload_bytes += payload.len();
            let decoded = form.light_static_change().unwrap().unwrap();
            assert_eq!(decoded.data_bytes(), payload.as_ref());
        }
    }
    assert_eq!(counts, [0, 0, 0]);
    assert_eq!(payload_bytes, 0);
}
