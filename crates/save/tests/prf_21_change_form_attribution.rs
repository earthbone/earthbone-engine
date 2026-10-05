// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-21 checks exact change-form byte attribution over the user's saves.

use std::path::{Path, PathBuf};

use soe_save::Save;

fn collect(directory: &Path, saves: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, saves);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            saves.push(path);
        }
    }
}

#[test]
fn prf_21_change_form_bytes_reconcile_by_plugin_and_record_type_across_user_saves() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 save survey");
        return;
    };
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("SOE_TEST_SAVES contains no .ess files, skipping PRF-21 save survey");
        return;
    }

    let mut surveyed = 0;
    for path in paths {
        let save = Save::open(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let attributed: usize = save
            .change_form_sizes_by_plugin()
            .iter()
            .map(|row| row.bytes)
            .sum();
        let serialized: usize = save
            .change_forms
            .iter()
            .map(|form| {
                let mut bytes = Vec::new();
                form.write(&mut bytes);
                bytes.len()
            })
            .sum();
        assert_eq!(attributed, serialized, "{}", path.display());
        surveyed += 1;
    }
    assert!(surveyed > 0);
}
