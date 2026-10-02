// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-13 requires saved package procedure state to reach live actor restore.

use soe_save::Save;
use std::path::{Path, PathBuf};

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

#[test]
fn sav_13_named_saved_package_states_decode_for_actor_restoration() {
    let root = std::env::var_os("SOE_TEST_SAVES")
        .expect("SOE_TEST_SAVES must point to the observed ESS corpus");
    let root = Path::new(&root);
    let mut paths = vec![];
    collect(root, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 60, "requires the complete observed corpus");

    let mut sequence = 0;
    let mut stacked = 0;
    let mut rejected = vec![];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            if let Err(error) = form.reference_contents(save.form_version) {
                let message = error.to_string();
                if message.contains("PackageData kind 18 procedure") {
                    if message.contains("Sequence") {
                        sequence += 1;
                    } else if message.contains("Stacked") {
                        stacked += 1;
                    }
                    rejected.push(format!("{} {:?}: {message}", path.display(), form.form));
                }
            }
        }
    }

    assert_eq!(sequence + stacked, 81, "observed named package state count changed");
    assert_eq!(sequence, 72, "observed Sequence package state count changed");
    assert_eq!(stacked, 9, "observed Stacked package state count changed");
    assert!(
        rejected.is_empty(),
        "named package procedure state must decode before it can resume on a live actor: {rejected:?}"
    );
}
