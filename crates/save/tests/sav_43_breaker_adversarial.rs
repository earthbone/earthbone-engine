// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{io::Write, path::{Path, PathBuf}};

use soe_save::{ChangeForm, RefId, Save};

#[test]
fn sav_43_compressed_payload_length_is_checked_on_repeated_decode() {
    for kind in [19, 27] {
        let raw = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut change = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: 0xFFFF_FFFF,
            type_field: kind,
            version: 78,
            uncompressed_len: raw.len() as u32,
            data: compressed.clone(),
        };

        for _ in 0..2 {
            let decoded = if kind == 19 {
                change.apparatus_change()
            } else {
                change.idle_marker_change()
            }
            .unwrap()
            .unwrap();
            assert_eq!(decoded.change_flags, u32::MAX);
            assert_eq!(decoded.data_bytes(), raw);
        }

        change.uncompressed_len += 1;
        let malformed = if kind == 19 {
            change.apparatus_change()
        } else {
            change.idle_marker_change()
        };
        assert!(malformed.is_err(), "type {kind} accepted a wrong zlib size");
    }
}

fn saves(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            saves(&path, output);
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ess")) {
            output.push(path);
        }
    }
}

#[test]
fn sav_43_external_corpus_accounts_for_every_matching_payload() {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || PathBuf::from("/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves"),
        PathBuf::from,
    );
    let mut paths = Vec::new();
    saves(&root, &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 saves");

    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            if matches!(form.record_type(), "APPA" | "IDLM") {
                let raw = form.payload().unwrap();
                let decoded = if form.record_type() == "APPA" {
                    form.apparatus_change().unwrap().unwrap()
                } else {
                    form.idle_marker_change().unwrap().unwrap()
                };
                assert_eq!(decoded.data_bytes(), raw.as_ref(), "{}", path.display());
            }
        }
    }
}
