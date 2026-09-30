// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-49 WOOP and MGEF change payload coverage.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use soe_esm::{Plugin, Tag};
use soe_save::{ChangeForm, RefId, Save};

const FORM: u32 = 1;
const VALUE: u32 = 1 << 1;
const FULLNAME: u32 = 1 << 2;
const WOOP: u8 = 34;
const MGEF: u8 = 35;

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn decode(form: &ChangeForm) -> soe_save::Result<Option<soe_save::BaseObjectMarkerChange>> {
    match form.record_type() {
        "WOOP" => form.word_of_power_change(),
        "MGEF" => form.magic_effect_change(),
        _ => Ok(None),
    }
}

#[test]
fn sav_49_named_flag_combinations_keep_typed_fields_and_exact_bytes() {
    for kind in [WOOP, MGEF] {
        for bits in 0_u8..8 {
            let flags = u32::from(bits);
            let mut data = Vec::new();
            if flags & FORM != 0 {
                data.extend([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
            }
            let form = change(kind, flags, data.clone());
            let typed = decode(&form).unwrap().unwrap();
            assert_eq!(typed.change_flags(), flags, "kind={kind} flags={flags:#x}");
            assert_eq!(typed.data_bytes(), data, "kind={kind} flags={flags:#x}");
            assert_eq!(typed.form_flags.is_some(), flags & FORM != 0);
            assert_eq!(typed.base_object_value_changed, flags & VALUE != 0);
            assert_eq!(typed.base_object_fullname_changed, flags & FULLNAME != 0);
        }
    }
}

#[test]
fn sav_49_unnamed_flags_and_form_flag_values_round_trip() {
    for kind in [WOOP, MGEF] {
        let flags = (1 << 0) | (1 << 2) | (1 << 7) | (1 << 31);
        let data = [0xef, 0xcd, 0xab, 0x89, 0x76, 0x54];
        let form = change(kind, flags, data.to_vec());
        let typed = decode(&form).unwrap().unwrap();
        assert_eq!(typed.unnamed_flags, flags & !7);
        assert_eq!(typed.form_flags.unwrap().flags, 0x89ab_cdef);
        assert_eq!(typed.form_flags.unwrap().unknown, 0x5476);
        assert_eq!(typed.change_flags(), flags);
        assert_eq!(typed.data_bytes(), data);
    }
}

#[test]
fn sav_49_truncation_trailing_and_wrong_record_inputs_fail_or_skip() {
    for kind in [WOOP, MGEF] {
        let full = change(kind, FORM, vec![1, 2, 3, 4, 5, 6]);
        for cut in 0..6 {
            let mut short = full.clone();
            short.data.truncate(cut);
            assert!(decode(&short).is_err(), "kind={kind} cut={cut}");
        }
        let mut trailing = full;
        trailing.data.push(0);
        assert!(decode(&trailing).is_err(), "kind={kind}");
    }
    assert!(decode(&change(0, 0, vec![])).unwrap().is_none());
    assert!(decode(&change(WOOP, 0, vec![0])).is_err());
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
fn sav_49_external_corpus_accounts_for_woop_and_mgef_payloads() {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || {
            PathBuf::from(
                "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves",
            )
        },
        PathBuf::from,
    );
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no external saves found, skipping SAV-49 corpus accounting");
        return;
    }
    assert!(
        paths.len() >= 50,
        "SAV-49 requires the full external corpus"
    );
    let mut variants = BTreeMap::new();
    let mut payload_bytes = 0_usize;
    let mut counts = BTreeMap::from([("WOOP", 0_usize), ("MGEF", 0)]);
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let kind = form.record_type();
            let Some(count) = counts.get_mut(kind) else {
                continue;
            };
            let decoded = decode(form)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} {} {:06x}: {error}",
                        path.display(),
                        kind,
                        form.form.raw()
                    )
                })
                .unwrap();
            let bytes = decoded.data_bytes();
            assert_eq!(decoded.change_flags(), form.change_flags);
            assert_eq!(bytes, form.payload().unwrap().as_ref());
            *count += 1;
            payload_bytes += bytes.len();
            *variants
                .entry((
                    save.form_version,
                    form.version,
                    kind,
                    form.change_flags,
                    bytes.len(),
                ))
                .or_insert(0_usize) += 1;
        }
    }
    eprintln!(
        "SAV-49 {} saves, {payload_bytes} payload bytes, counts={counts:?}",
        paths.len()
    );
    for ((save_version, form_version, kind, flags, bytes), count) in variants {
        eprintln!("save_version={save_version} form_version={form_version} type={kind} flags={flags:#010x} bytes={bytes} count={count}");
    }
}

#[test]
fn sav_49_installed_skyrim_has_both_vanilla_record_types() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping Skyrim.esm signature check");
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    for tag in [Tag::new(b"WOOP"), Tag::new(b"MGEF")] {
        assert!(
            plugin.records.iter().any(|record| record.tag == tag),
            "missing {tag}"
        );
    }
}
