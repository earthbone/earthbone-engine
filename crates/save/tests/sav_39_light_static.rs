// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-39 LIGH, STAT and MSTT layouts from xEdit save definitions.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use soe_save::quest::FormFlags;
use soe_save::{ChangeForm, RefId, Save};

const KINDS: [u8; 3] = [17, 20, 21];

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0, 1, 0x23]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn payload(flags: u32) -> Vec<u8> {
    if flags & 1 == 0 {
        Vec::new()
    } else {
        vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
    }
}

#[test]
fn sav_39_every_source_flag_combination_roundtrips_exactly() {
    for kind in KINDS {
        for flags in 0..=7 {
            let bytes = payload(flags);
            let change = form(kind, flags, bytes.clone());
            let decoded = change.light_static_change().unwrap().unwrap();
            assert_eq!(
                decoded.change_flags(),
                flags,
                "kind {kind}, flags {flags:#x}"
            );
            assert_eq!(decoded.data_bytes(), bytes, "kind {kind}, flags {flags:#x}");
            assert_eq!(
                decoded.form_flags,
                (flags & 1 != 0).then_some(FormFlags {
                    flags: 0xB4A3_9281,
                    unknown: 0xD6C5,
                }),
                "kind {kind}, flags {flags:#x}"
            );
            assert_eq!(decoded.value_changed, flags & 2 != 0);
            assert_eq!(decoded.full_name_changed, flags & 4 != 0);
        }
    }
}

#[test]
fn sav_39_every_truncated_flag_payload_is_rejected() {
    for kind in KINDS {
        let complete = payload(1);
        for cut in 0..complete.len() {
            assert!(
                form(kind, 1, complete[..cut].to_vec())
                    .light_static_change()
                    .is_err(),
                "kind {kind}, cut {cut}"
            );
        }
    }
}

#[test]
fn sav_39_rejects_undefined_flags_and_trailing_payload() {
    for kind in KINDS {
        assert!(form(kind, 8, Vec::new()).light_static_change().is_err());
        assert!(form(kind, 0, vec![0xA5]).light_static_change().is_err());
    }
}

#[test]
fn sav_39_other_record_types_remain_opaque() {
    assert!(form(18, u32::MAX, vec![0xA5])
        .light_static_change()
        .unwrap()
        .is_none());
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
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
fn sav_39_external_corpus_accounts_for_all_light_static_changes() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping LIGH/STAT/MSTT corpus survey");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "survey requires at least 50 saves");

    let mut counts = BTreeMap::from([("LIGH", 0usize), ("STAT", 0), ("MSTT", 0)]);
    let mut flag_sets: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let kind = change.record_type();
            let Some(count) = counts.get_mut(kind) else {
                continue;
            };
            *count += 1;
            payload_bytes += change.payload().unwrap().len();
            let decoded = change.light_static_change().unwrap().unwrap();
            assert_eq!(decoded.change_flags(), change.change_flags);
            assert_eq!(decoded.data_bytes(), change.payload().unwrap().as_ref());
            flag_sets
                .entry(kind)
                .or_default()
                .insert(change.change_flags);
        }
    }
    eprintln!(
        "SAV-39 corpus: {} saves, records {counts:?}, flag sets {flag_sets:?}, {payload_bytes} payload bytes",
        paths.len()
    );
}
