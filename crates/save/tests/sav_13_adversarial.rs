// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-13 requires saved package procedure state to reach live actor restore.

use soe_save::{reference::Field, Save};
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
    let mut decoded = 0;
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let Ok(Some(contents)) = form.reference_contents(save.form_version) else {
                continue;
            };
            let Some(extra) = contents.extra.as_ref() else {
                continue;
            };
            for entry in &extra.entries {
                if entry.kind != 108 || !matches!(entry.fields.first(), Some(Field::Byte(18))) {
                    continue;
                }
                match entry.fields.get(2) {
                    Some(Field::String(name)) if name.bytes() == b"Sequence" => {
                        assert!(matches!(entry.fields.get(3), Some(Field::Group(_))));
                        sequence += 1;
                        decoded += 1;
                    }
                    Some(Field::String(name)) if name.bytes() == b"Stacked" => {
                        assert!(matches!(entry.fields.get(3), Some(Field::Group(_))));
                        stacked += 1;
                        decoded += 1;
                    }
                    _ => {}
                }
            }
        }
    }

    assert_eq!(decoded, 81, "observed named package state count changed");
    assert_eq!(
        sequence, 72,
        "observed Sequence package state count changed"
    );
    assert_eq!(stacked, 9, "observed Stacked package state count changed");
}
