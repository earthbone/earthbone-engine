// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_save::{ChangeForm, RefId, Save};

#[test]
fn sav_41_every_unnamed_bit_roundtrips_with_each_named_flag_subset() {
    for kind in [14, 18, 22] {
        for bit in 3..32 {
            for named in 0..8_u32 {
                let flags = (1_u32 << bit) | named;
                let mut payload = vec![0xDE, 0xAD];
                if flags & 1 != 0 {
                    payload.splice(0..0, [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]);
                }
                let change = ChangeForm {
                    form: RefId::from_bytes([0x40, 1, 2]),
                    change_flags: flags,
                    type_field: kind,
                    version: 78,
                    uncompressed_len: 0,
                    data: payload.clone(),
                };
                let decoded = change.static_object_change().unwrap().unwrap();
                assert_eq!(decoded.change_flags().unwrap(), flags);
                assert_eq!(decoded.unnamed_flags, flags & !7);
                assert_eq!(decoded.data_bytes().unwrap(), payload);
            }
        }
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
fn sav_41_external_corpus_accounts_for_every_matching_payload() {
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
            if matches!(form.record_type(), "CONT" | "FURN" | "MISC") {
                let raw = form.payload().unwrap();
                let decoded = form.static_object_change().unwrap().unwrap();
                assert_eq!(decoded.data_bytes().unwrap(), raw.as_ref(), "{}", path.display());
            }
        }
    }
}
