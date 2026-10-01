// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_save::{ChangeForm, RefId, Save};

fn form(kind: u8, flags: u32, payload: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data: payload,
    }
}

#[test]
fn sav_39_xedit_unnamed_flags_roundtrip_for_all_three_types() {
    for kind in [17, 20, 21] {
        for bit in 3..32 {
            let flags = 1_u32 << bit;
            let payload = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
            let decoded = form(kind, flags | 1, payload.to_vec()).light_static_change();
            assert!(decoded.is_ok(), "type {kind} rejected xEdit unnamed bit {bit}");
            let decoded = decoded.unwrap().unwrap();
            assert_eq!(decoded.data_bytes(), payload, "type {kind}, bit {bit}");
            assert_eq!(decoded.change_flags(), flags | 1, "type {kind}, bit {bit}");
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
fn sav_39_external_corpus_accounts_for_every_matching_payload() {
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
            if matches!(form.record_type(), "LIGH" | "STAT" | "MSTT") {
                let raw = form.payload().unwrap();
                let decoded = form.light_static_change().unwrap().unwrap();
                assert_eq!(decoded.data_bytes(), raw.as_ref(), "{}", path.display());
            }
        }
    }
}
