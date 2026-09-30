// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Coverage of saved spatial records without assigning meanings to opaque bytes.

use soe_save::{Error, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

#[derive(Default)]
struct Coverage {
    records: usize,
    bytes: usize,
    compressed: usize,
    lengths: BTreeMap<usize, usize>,
}

#[test]
fn sav_27_corpus_accounts_for_spatial_changes_without_guessing_fields() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping spatial change coverage");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    // Corpus breadth is required before accepting the first measurement.
    assert!(paths.len() >= 50);
    let mut totals = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut groups = BTreeMap::<(&str, u8, u8, u32), Coverage>::new();
    let mut typed = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut opaque_or_unsupported = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut total_changes = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        let rewritten = Save::parse(&save.write().unwrap()).unwrap();
        assert_eq!(rewritten.change_forms.len(), save.change_forms.len());
        total_changes += save.change_forms.len();
        for (change, again) in save.change_forms.iter().zip(&rewritten.change_forms) {
            let Some(total) = totals.get_mut(change.record_type()) else {
                continue;
            };
            *total += 1;
            assert_eq!(again.form, change.form);
            assert_eq!(again.type_field, change.type_field);
            assert_eq!(again.version, change.version);
            assert_eq!(again.change_flags, change.change_flags);
            assert_eq!(again.uncompressed_len, change.uncompressed_len);
            assert_eq!(again.data, change.data);
            let payload = change.payload().unwrap();
            assert_eq!(again.payload().unwrap(), payload);
            let decoded_bytes = match change.record_type() {
                "CELL" => change
                    .cell_change()
                    .map(|change| change.map(|value| value.data_bytes())),
                "LCTN" => change
                    .location_change()
                    .map(|change| change.map(|value| value.data_bytes())),
                "ECZN" => change
                    .encounter_zone_change()
                    .map(|change| change.map(|value| value.data_bytes())),
                _ => unreachable!(),
            };
            match decoded_bytes {
                Ok(Some(bytes)) => {
                    assert_eq!(
                        bytes,
                        payload.as_ref(),
                        "{} flags={:#010x}",
                        change.record_type(),
                        change.change_flags
                    );
                    *typed.get_mut(change.record_type()).unwrap() += 1;
                }
                Ok(None) => panic!("spatial decoder declined {}", change.record_type()),
                Err(Error::SpatialChange { .. }) => {
                    *opaque_or_unsupported.get_mut(change.record_type()).unwrap() += 1;
                }
                Err(error) => panic!(
                    "{} flags={:#010x}: {error}",
                    change.record_type(),
                    change.change_flags
                ),
            }
            let group = groups
                .entry((
                    change.record_type(),
                    save.form_version,
                    change.version,
                    change.change_flags,
                ))
                .or_default();
            group.records += 1;
            group.bytes += payload.len();
            group.compressed += usize::from(change.is_compressed());
            *group.lengths.entry(payload.len()).or_default() += 1;
        }
    }
    assert_eq!(
        groups.values().map(|group| group.records).sum::<usize>(),
        totals.values().sum::<usize>()
    );
    eprintln!(
        "saves={} total_change_forms={total_changes} targets={totals:?} typed={typed:?} opaque_or_unsupported={opaque_or_unsupported:?}",
        paths.len()
    );
    for ((kind, save_version, change_version, flags), group) in groups {
        eprintln!(
            "{kind} save_version={save_version} change_version={change_version} flags={flags:08X} records={} bytes={} compressed={} lengths={:?}",
            group.records, group.bytes, group.compressed, group.lengths
        );
    }
}
