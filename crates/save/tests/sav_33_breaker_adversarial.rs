// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId};
use std::path::{Path, PathBuf};

const CORPUS: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_33_breaker_all_item_types_preserve_tail_without_form_flags() {
    for (kind, name) in [(12, "ARMO"), (23, "WEAP"), (24, "AMMO")] {
        let bytes = [0x00, 0xFF, 0x00, 0x81];
        let change = form(kind, 0x8000_0006, bytes.to_vec());
        let decoded = change.item_change().unwrap().unwrap();
        assert_eq!(decoded.record_type, name);
        assert_eq!(decoded.form_flags, None);
        assert!(decoded.value_changed && decoded.full_name_changed);
        assert_eq!(decoded.unnamed_flags, 0x8000_0000);
        assert_eq!(decoded.data_bytes(), bytes);
        assert_eq!(decoded.change_flags(), change.change_flags);
    }
}

#[test]
fn sav_33_breaker_mixed_type_index_bits_do_not_change_dispatch() {
    for kind in [12, 23, 24] {
        let change = form(kind | 0xC0, 0, vec![]);
        assert!(change.item_change().unwrap().is_some());
    }
    for kind in [11, 13, 22, 25] {
        let change = form(kind | 0xC0, u32::MAX, vec![0xFF]);
        assert!(change.item_change().unwrap().is_none());
    }
}

fn collect(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, output);
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ess")) {
            output.push(path);
        }
    }
}

#[test]
fn sav_33_breaker_external_corpus_accounts_all_matching_forms() {
    let mut paths = Vec::new();
    collect(Path::new(CORPUS), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let mut counts = [0usize; 3];
    for path in paths {
        let save = soe_save::Save::open(&path).unwrap();
        for change in &save.change_forms {
            let Some(index) = ["ARMO", "WEAP", "AMMO"]
                .iter()
                .position(|name| change.record_type() == *name)
            else {
                continue;
            };
            let decoded = change.item_change().unwrap().unwrap();
            assert_eq!(decoded.change_flags(), change.change_flags);
            assert_eq!(decoded.data_bytes(), change.payload().unwrap().as_ref());
            counts[index] += 1;
        }
    }
    eprintln!("SAV-33 corpus ARMO/WEAP/AMMO counts={counts:?}");
}
