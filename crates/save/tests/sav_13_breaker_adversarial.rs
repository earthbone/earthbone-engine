// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-13 package frames must remain bounded through reference-content decoding.

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
fn sav_13_sequence_and_stacked_frames_roundtrip_before_live_restore() {
    let root = std::env::var_os("SOE_TEST_SAVES")
        .expect("SOE_TEST_SAVES must point to the observed ESS corpus");
    let mut paths = vec![];
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 60, "requires the complete observed corpus");

    let mut sequence = 0;
    let mut stacked = 0;
    for path in paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let Ok(Some(contents)) = change.reference_contents(save.form_version) else {
                continue;
            };
            let Some(extra) = contents.extra.as_ref() else {
                continue;
            };
            for entry in &extra.entries {
                if entry.kind != 108 || !matches!(entry.fields.first(), Some(Field::Byte(18))) {
                    continue;
                }
                let Some(Field::Group(_)) = entry.fields.get(3) else {
                    continue;
                };
                match entry.fields.get(2) {
                    Some(Field::String(name)) if name.bytes() == b"Sequence" => sequence += 1,
                    Some(Field::String(name)) if name.bytes() == b"Stacked" => stacked += 1,
                    _ => continue,
                }
                assert_eq!(
                    contents.data_bytes().unwrap(),
                    change.data[contents.start..contents.end]
                );
                let repeated = change
                    .reference_contents(save.form_version)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    repeated.data_bytes().unwrap(),
                    contents.data_bytes().unwrap()
                );
            }
        }
    }

    assert_eq!(sequence, 72);
    assert_eq!(stacked, 9);
}
