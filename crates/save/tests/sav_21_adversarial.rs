// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Stacked package-state acceptance over the observed save corpus.

use soe_save::{reference::ExtraData, reference::Field, Save};
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

fn stacked_count(extra: &ExtraData) -> usize {
    extra
        .entries
        .iter()
        .filter(|entry| {
            entry.kind == 108
                && matches!(
                    entry.fields.as_slice(),
                    [Field::Byte(18), Field::Byte(0), Field::String(name), Field::Group(_)]
                        if name.bytes() == b"Stacked"
                )
        })
        .count()
}

#[test]
fn all_observed_stacked_payloads_roundtrip_and_keep_the_following_boundary() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Stacked package corpus");
        return;
    };
    let root = Path::new(&root);
    let mut paths = vec![];
    collect(root, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 60, "requires the complete observed corpus");

    let (mut stacked, mut rejected) = (0, 0);
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            let contents = match form.reference_contents(save.form_version) {
                Ok(Some(contents)) => contents,
                Err(error) if error.to_string().contains("Stacked") => {
                    stacked += 1;
                    rejected += 1;
                    continue;
                }
                Ok(None) | Err(_) => continue,
            };
            let mut count = contents.extra.as_ref().map_or(0, stacked_count);
            if let Some(inventory) = &contents.inventory {
                count += inventory
                    .entries
                    .iter()
                    .flat_map(|entry| &entry.stacks)
                    .map(stacked_count)
                    .sum::<usize>();
            }
            if count == 0 {
                continue;
            }
            stacked += count;
            let payload = form.payload().unwrap();
            let decoded = contents.data_bytes().unwrap();
            assert_eq!(decoded, payload[contents.start..contents.end]);
            assert_eq!(contents.start + decoded.len(), contents.end);
            assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
        }
    }

    assert_eq!(stacked, 9, "observed Stacked occurrence count changed");
    assert_eq!(rejected, 0, "every observed Stacked payload must decode");
}
