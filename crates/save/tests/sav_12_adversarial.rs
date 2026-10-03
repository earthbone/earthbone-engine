// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-12 requires typed Sequence payloads with exact following boundaries.

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
fn sav_12_observed_sequence_payloads_round_trip_with_the_following_boundary() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-12 corpus check");
        return;
    };
    let root = Path::new(&root);
    let mut paths = vec![];
    collect(root, &mut paths);
    paths.sort();

    let mut sequence_count = 0;
    let mut failures = vec![];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            match form.reference_contents(save.form_version) {
                Ok(Some(contents)) => {
                    let payload = form.payload().unwrap();
                    assert_eq!(
                        contents.data_bytes().unwrap(),
                        payload[contents.start..contents.end],
                        "{} {:?}: decoded boundary did not re-encode",
                        path.display(),
                        form.form
                    );
                    if contents.extra.as_ref().is_some_and(|extra| {
                        extra.entries.iter().any(|entry| {
                            entry.kind == 108
                                && entry.fields.iter().any(|field| {
                                    matches!(field, Field::String(name) if name.bytes() == b"Sequence")
                                })
                        })
                    }) {
                        sequence_count += 1;
                    }
                }
                Err(error) if !error.to_string().contains("procedure \"Stacked\"") => {
                    sequence_count += 1;
                    failures.push(format!("{} {:?}: {error}", path.display(), form.form));
                }
                Ok(None) | Err(_) => {}
            }
        }
    }

    assert_eq!(sequence_count, 72, "known corpus Sequence count changed");
    assert!(
        failures.is_empty(),
        "Sequence payloads must decode and retain their following boundary: {failures:?}"
    );
}
