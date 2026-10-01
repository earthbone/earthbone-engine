// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-33 coverage survey, using xEdit change types 12, 23 and 24.

use std::{collections::BTreeMap, path::Path};

use soe_save::Save;

const CORPUS: &str =
    "/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

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
fn sav_33_corpus_reports_item_change_field_coverage() {
    let root = std::env::var_os("SOE_TEST_SAVES").map_or_else(
        || std::path::PathBuf::from(CORPUS),
        std::path::PathBuf::from,
    );
    if !root.is_dir() {
        eprintln!("external save corpus not found, skipping item change coverage survey");
        return;
    }
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "coverage survey requires at least 50 saves"
    );
    let mut counts = BTreeMap::from([("AMMO", 0usize), ("ARMO", 0), ("WEAP", 0)]);
    let mut variants = BTreeMap::new();
    let mut bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let kind = form.record_type();
            let Some(count) = counts.get_mut(kind) else {
                continue;
            };
            *count += 1;
            let payload = form.payload().unwrap();
            let decoded = form.item_change().unwrap().unwrap();
            assert_eq!(decoded.data_bytes(), payload.as_ref());
            assert_eq!(decoded.change_flags(), form.change_flags);
            bytes += payload.len();
            *variants
                .entry((
                    save.form_version,
                    form.version,
                    kind,
                    form.change_flags,
                    payload.len(),
                ))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-33 coverage schema 1: {} saves, {bytes} payload bytes",
        paths.len()
    );
    eprintln!("type counts: {counts:?}");
    for ((save_version, change_version, kind, flags, len), count) in variants {
        eprintln!("save_version={save_version} change_version={change_version} type={kind} flags={flags:#010x} payload_bytes={len} count={count}");
    }
    eprintln!("Every observed item payload decoded and re-encoded exactly.");
}
