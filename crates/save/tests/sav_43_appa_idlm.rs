// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-43 APPA and IDLM saved change fields from xEdit's save definitions.

use std::{collections::BTreeMap, path::Path};

use soe_save::{ChangeForm, RefId, Save};

const FORM_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn change(kind: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn decoded(form: &ChangeForm) -> soe_save::Result<Option<soe_save::AppaIdleMarkerChange>> {
    match form.record_type() {
        "APPA" => form.apparatus_change(),
        "IDLM" => form.idle_marker_change(),
        _ => Ok(None),
    }
}

#[test]
fn sav_43_source_flag_combinations_retain_typed_fields_and_exact_bytes() {
    for kind in [19u8, 27] {
        for flags in 0..8u32 {
            let payload = if flags & 1 != 0 {
                FORM_FLAGS.to_vec()
            } else {
                Vec::new()
            };
            let value = decoded(&change(kind, flags, payload.clone()))
                .unwrap()
                .unwrap();
            assert_eq!(value.change_flags, flags, "type {kind}, flags {flags:#x}");
            assert_eq!(value.form_flags.is_some(), flags & 1 != 0);
            if flags & 1 != 0 {
                assert_eq!(value.form_flags, Some((0xB4A3_9281, 0xD6C5)));
            }
            assert_eq!(value.value_changed, flags & 2 != 0);
            assert_eq!(value.full_name_changed, flags & 4 != 0);
            assert_eq!(value.data_bytes(), payload, "type {kind}, flags {flags:#x}");
        }
    }
}

#[test]
fn sav_43_preserves_each_unnamed_flag_with_every_named_combination() {
    let unnamed_mask = !0b111u32;
    let flag_words = (0..8u32)
        .flat_map(|named| {
            (3..32)
                .map(move |bit| named | (1u32 << bit))
                .chain(std::iter::once(named | unnamed_mask))
        })
        .collect::<Vec<_>>();

    for kind in [19u8, 27] {
        for flags in flag_words.iter().copied() {
            let payload = if flags & 1 != 0 {
                FORM_FLAGS.to_vec()
            } else {
                Vec::new()
            };
            let value = decoded(&change(kind, flags, payload.clone()))
                .unwrap()
                .unwrap();
            assert_eq!(value.change_flags, flags, "type {kind}, flags {flags:#x}");
            assert_eq!(value.data_bytes(), payload, "type {kind}, flags {flags:#x}");
            assert_eq!(value.form_flags.is_some(), flags & 1 != 0);
            assert_eq!(value.value_changed, flags & 2 != 0);
            assert_eq!(value.full_name_changed, flags & 4 != 0);
        }
    }
}

#[test]
fn sav_43_form_flags_reject_every_truncated_prefix() {
    for kind in [19u8, 27] {
        for prefix in 0..FORM_FLAGS.len() {
            assert!(
                decoded(&change(kind, 1, FORM_FLAGS[..prefix].to_vec())).is_err(),
                "type {kind}, prefix length {prefix}"
            );
        }
    }
}

#[test]
fn sav_43_rejects_trailing_bytes_and_preserves_unnamed_flag_bits() {
    for kind in [19u8, 27] {
        assert!(decoded(&change(kind, 0, vec![0xA5])).is_err());
        let flags = 0x8000_0000 | 2;
        let value = decoded(&change(kind, flags, Vec::new())).unwrap().unwrap();
        assert_eq!(value.change_flags, flags);
        assert!(value.value_changed);
    }
    assert!(decoded(&change(20, 0, Vec::new())).unwrap().is_none());
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
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
fn sav_43_external_corpus_accounts_for_every_appa_and_idlm_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping APPA/IDLM corpus survey");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "survey requires at least 50 external saves"
    );

    let mut counts = BTreeMap::from([("APPA", 0usize), ("IDLM", 0usize)]);
    let mut flag_counts = BTreeMap::new();
    let mut payload_bytes = 0usize;
    let mut total_change_forms = 0usize;
    let mut accounted_change_forms = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            total_change_forms += 1;
            let kind = form.record_type();
            let Some(count) = counts.get_mut(kind) else {
                continue;
            };
            accounted_change_forms += 1;
            *count += 1;
            let value = decoded(form).unwrap().unwrap();
            let payload = form.payload().unwrap();
            assert_eq!(value.data_bytes(), payload.as_ref(), "{}", path.display());
            payload_bytes += payload.len();
            *flag_counts
                .entry((kind, form.change_flags))
                .or_insert(0usize) += 1;
        }
    }
    assert_eq!(
        accounted_change_forms,
        counts.values().sum(),
        "every APPA and IDLM change form must be included in the totals"
    );
    eprintln!(
        "SAV-43 corpus: {} saves, {total_change_forms} total change forms, {accounted_change_forms} APPA/IDLM forms, counts {counts:?}, flags {flag_counts:?}, {payload_bytes} payload bytes",
        paths.len(),
    );
}
