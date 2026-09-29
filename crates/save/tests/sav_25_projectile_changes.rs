// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-25 PCON and PARW envelopes from xEdit and external save coverage.

use std::{collections::BTreeMap, path::Path};

use soe_save::{ChangeForm, RefId, Save};

fn change(kind: u8, flags: u32, version: u8, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0, 0, 0]),
        change_flags: flags,
        type_field: kind,
        version,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_25_pcon_and_parw_preserve_each_flag_and_payload_byte() {
    let flags_to_check = std::iter::once(0)
        .chain(std::iter::once(u32::MAX))
        .chain((0..32).map(|bit| 1_u32 << bit));
    for kind in [42, 47] {
        for flags in flags_to_check.clone() {
            let data: Vec<u8> = flags
                .to_le_bytes()
                .into_iter()
                .chain([0xA5, 0, 0x5A])
                .collect();
            let original = change(kind, flags, 74, data.clone());
            let decoded = if kind == 42 {
                let value = original.pcon_change().unwrap();
                assert_eq!(value.change_flags, flags);
                assert_eq!(value.version, 74);
                assert_eq!(value.uncompressed_len, 0);
                assert_eq!(value.data, data);
                value.data
            } else {
                let value = original.parw_change().unwrap();
                assert_eq!(value.change_flags, flags);
                assert_eq!(value.version, 74);
                assert_eq!(value.uncompressed_len, 0);
                assert_eq!(value.data, data);
                value.data
            };
            assert_eq!(decoded, original.data);
        }
    }
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn sav_25_external_corpus_accounts_for_pcon_and_parw_bytes() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-25 save census");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "SAV-25 census requires at least 50 saves"
    );

    let mut totals = BTreeMap::<&str, (usize, usize, BTreeMap<(u8, u32), usize>)>::new();
    for tag in ["PCON", "PARW"] {
        totals.insert(tag, (0, 0, BTreeMap::new()));
    }
    let mut save_change_forms = 0_usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        save_change_forms += save.change_forms.len();
        for form in &save.change_forms {
            let tag = form.record_type();
            let Some((records, bytes, variants)) = totals.get_mut(tag) else {
                continue;
            };
            let decoded = match tag {
                "PCON" => {
                    let value = form.pcon_change().unwrap();
                    (
                        value.change_flags,
                        value.version,
                        value.uncompressed_len,
                        value.data,
                    )
                }
                "PARW" => {
                    let value = form.parw_change().unwrap();
                    (
                        value.change_flags,
                        value.version,
                        value.uncompressed_len,
                        value.data,
                    )
                }
                _ => continue,
            };
            assert_eq!(decoded.0, form.change_flags);
            assert_eq!(decoded.1, form.version);
            assert_eq!(decoded.2, form.uncompressed_len);
            assert_eq!(decoded.3, form.data);
            *records += 1;
            *bytes += form.data.len();
            *variants
                .entry((form.version, form.change_flags))
                .or_default() += 1;
        }
    }
    assert!(
        save_change_forms > 100_000,
        "SAV-25 census must inspect the full save corpus"
    );
    eprintln!(
        "SAV-25 schema 1: {} saves, {save_change_forms} change forms",
        paths.len()
    );
    for (tag, (records, bytes, variants)) in totals {
        eprintln!("{tag}: {records} records, {bytes} stored bytes, {variants:?} version and flag combinations");
    }
}
