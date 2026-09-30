// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-41 static-object payloads from xEdit's Skyrim save definitions.

use std::{collections::BTreeMap, path::Path};

use soe_save::base_object::static_object_flags as flags;
use soe_save::{ChangeForm, RefId, Save};

const TYPES: [(u8, &str); 3] = [(14, "CONT"), (18, "MISC"), (22, "FURN")];

fn form(kind: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x01, 0x23]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn fixture(change_flags: u32, suffix: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    if change_flags & flags::FORM != 0 {
        data.extend_from_slice(&[0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]);
    }
    data.extend_from_slice(suffix);
    data
}

#[test]
fn sav_41_source_flag_subsets_round_trip_for_each_record_type() {
    for (kind, record_type) in TYPES {
        for subset in 0..8 {
            let change_flags = [flags::FORM, flags::VALUE, flags::FULL_NAME]
                .into_iter()
                .enumerate()
                .fold(1 << 12, |all_flags, (bit, flag)| {
                    all_flags | if subset & (1 << bit) != 0 { flag } else { 0 }
                });
            let data = fixture(change_flags, &[0xDE, 0xAD]);
            let change = form(kind, change_flags, data.clone());
            let decoded = change.static_object_change().unwrap().unwrap();

            assert_eq!(change.record_type(), record_type);
            assert_eq!(decoded.change_flags, change_flags);
            assert_eq!(
                decoded.form_flags.is_some(),
                change_flags & flags::FORM != 0
            );
            assert_eq!(decoded.value_changed, change_flags & flags::VALUE != 0);
            assert_eq!(
                decoded.full_name_changed,
                change_flags & flags::FULL_NAME != 0
            );
            assert_eq!(decoded.data_bytes().unwrap(), data);
        }
    }
}

#[test]
fn sav_41_form_flag_rejects_every_truncated_prefix() {
    for (kind, record_type) in TYPES {
        for cut in 0..6 {
            let change = form(kind, flags::FORM, vec![0xA5; cut]);
            assert!(
                change.static_object_change().is_err(),
                "{record_type} accepted a {cut}-byte form-flags prefix"
            );
        }
    }
}

#[test]
fn sav_41_non_target_records_remain_opaque() {
    let change = form(12, u32::MAX, vec![0xA5]);
    assert!(change.static_object_change().unwrap().is_none());
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_41_external_corpus_accounts_for_static_object_payloads() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-41 external save survey");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("CONT", 0usize), ("FURN", 0), ("MISC", 0)]);
    let mut variants = BTreeMap::new();
    let mut payload_bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let record_type = change.record_type();
            let Some(count) = counts.get_mut(record_type) else {
                continue;
            };
            *count += 1;
            let decoded = change.static_object_change().unwrap().unwrap();
            let raw = change.payload().unwrap();
            assert_eq!(decoded.data_bytes().unwrap(), raw.as_ref());
            payload_bytes += raw.len();
            *variants
                .entry((
                    save.form_version,
                    change.version,
                    record_type,
                    change.change_flags,
                ))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-41 coverage: {} saves, {payload_bytes} CONT/FURN/MISC payload bytes, record counts {counts:?}",
        paths.len()
    );
    for ((save_version, change_version, record_type, change_flags), count) in variants {
        eprintln!("save_version={save_version} change_version={change_version} type={record_type} flags={change_flags:#010x} count={count}");
    }
}
