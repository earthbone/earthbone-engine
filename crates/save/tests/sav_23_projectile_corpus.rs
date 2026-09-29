// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Accounts for PFLA, PHZD and PBAR payloads in the external save corpus.

use soe_save::Save;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const CORPUS: &str =
    "/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
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
fn sav_23_projectile_payloads_report_external_corpus_accounting() {
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    let mut paths = vec![];
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no external saves found, set SOE_TEST_SAVES, skipping SAV-23 corpus survey");
        return;
    }
    assert!(
        paths.len() >= 50,
        "SAV-23 survey requires the external corpus"
    );

    let mut totals = BTreeMap::<&str, (usize, usize)>::new();
    let mut variants = BTreeMap::<(&str, u8, u8, u32), (usize, usize)>::new();
    let mut change_forms = 0_usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        change_forms += save.change_forms.len();
        for change in &save.change_forms {
            let Some(decoded) = change.projectile_change().unwrap() else {
                continue;
            };
            let name = change.record_type();
            let total = totals.entry(name).or_default();
            total.0 += 1;
            total.1 += decoded.payload.len();
            let variant = variants
                .entry((
                    name,
                    save.form_version,
                    change.version,
                    decoded.flags.bits(),
                ))
                .or_default();
            variant.0 += 1;
            variant.1 += decoded.payload.len();

            let mut encoded = Vec::new();
            change.write(&mut encoded);
            assert!(encoded.ends_with(&change.data));
            assert_eq!(
                u32::from_le_bytes(encoded[3..7].try_into().unwrap()),
                decoded.flags.bits()
            );
        }
    }
    assert!(
        change_forms > 100_000,
        "survey must inspect all change forms"
    );
    eprintln!("{} saves, {change_forms} change forms", paths.len());
    for name in ["PFLA", "PHZD", "PBAR"] {
        let (count, bytes) = totals.get(name).copied().unwrap_or_default();
        eprintln!("{name}: {count} payloads, {bytes} inflated bytes");
    }
    for ((name, form_version, change_version, flags), (count, bytes)) in variants {
        eprintln!(
            "{name} form={form_version} change={change_version} flags={flags:08x}: {count} payloads, {bytes} inflated bytes"
        );
    }
}
