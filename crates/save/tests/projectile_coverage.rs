// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Counts saved projectile change forms without assuming an undocumented layout.

use soe_save::Save;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn sav_11_external_corpus_accounts_for_projectile_types_flags_and_versions() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping projectile coverage census");
        return;
    };
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "survey requires the external save corpus"
    );
    let mut changes = 0_usize;
    let mut totals = [(0_usize, 0_usize); 3];
    let mut variants = BTreeMap::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        changes += save.change_forms.len();
        for change in &save.change_forms {
            if !matches!(change.type_index(), 2..=4) {
                continue;
            }
            let payload = change.payload().unwrap();
            let index = usize::from(change.type_index() - 2);
            totals[index].0 += 1;
            totals[index].1 += payload.len();
            let key = (
                change.record_type(),
                save.header.version,
                save.form_version,
                change.version,
                change.change_flags,
            );
            let entry = variants.entry(key).or_insert((0_usize, 0_usize));
            entry.0 += 1;
            entry.1 += payload.len();
        }
    }
    assert!(
        changes > 100_000,
        "survey must inspect the full change table"
    );
    assert!(totals[0].0 > 0, "survey did not account for PMIS forms");
    assert!(totals[1].0 > 0, "survey did not account for PGRE forms");
    eprintln!("{} saves, {changes} change forms inspected", paths.len());
    for (name, (count, bytes)) in ["PMIS", "PGRE", "PBEA"].into_iter().zip(totals) {
        eprintln!("{name}: {count} changes, {bytes} decompressed payload bytes");
    }
    for ((name, file_version, form_version, change_version, flags), (count, bytes)) in variants {
        eprintln!(
            "{name} file={file_version} form={form_version} change={change_version} flags={flags:08x}: {count} changes, {bytes} bytes"
        );
    }
}
