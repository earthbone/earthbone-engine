// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for the saved Sequence payload framing.

use soe_save::{reference::Field, ChangeForm, RefId, Save, WString};
use std::path::{Path, PathBuf};

fn collect_saves(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

fn sequence_form(body: &[u8]) -> ChangeForm {
    let mut data = vec![4, 108, 18, 0];
    WString::from_text("Sequence").write(&mut data);
    data.extend(body);
    ChangeForm {
        form: RefId::from_bytes([0, 0, 1]),
        change_flags: 1 << 11,
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_12_invalid_child_followed_by_a_known_extra_is_rejected() {
    let form = sequence_form(&[0x19, 0x08, 0x85, 0x00, 0x09, 0x00]);
    let first = form.reference_contents(78).unwrap_err().to_string();
    let second = form.reference_contents(78).unwrap_err().to_string();
    assert!(matches!(
        form.reference_contents(78),
        Err(soe_save::Error::Truncated { .. })
    ));
    assert_eq!(first, second);
}

#[test]
fn sav_12_outer_terminator_inside_an_invalid_child_cannot_end_the_entry() {
    let form = sequence_form(&[0x11, 0x22, 0xFF, 0x85, 0x00]);
    let error = form.reference_contents(78).unwrap_err().to_string();
    assert!(error.contains("truncated"), "{error}");
}

#[test]
fn sav_12_every_truncated_sequence_name_fails_without_accepting_a_partial_boundary() {
    let complete = sequence_form(&[]);
    for cut in 4..complete.data.len() {
        let mut truncated = complete.clone();
        truncated.data.truncate(cut);
        assert!(truncated.reference_contents(78).is_err(), "cut {cut}");
    }
}

#[test]
fn sav_12_all_observed_sequence_payloads_reencode_and_preserve_the_following_boundary() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Sequence corpus check");
        return;
    };
    let mut paths = vec![];
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();

    let (mut found, mut failures) = (0, vec![]);
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            match form.reference_contents(save.form_version) {
                Ok(Some(contents)) => {
                    let is_sequence = contents.extra.as_ref().is_some_and(|extra| {
                        extra.entries.iter().any(|entry| {
                            entry.kind == 108
                                && entry.fields.iter().any(|field| {
                                    matches!(field, Field::String(name) if name.bytes() == b"Sequence")
                                })
                        })
                    });
                    if is_sequence {
                        found += 1;
                        let bytes = contents.data_bytes().unwrap();
                        let payload = form.payload().unwrap();
                        assert_eq!(bytes, payload[contents.start..contents.end]);
                        assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
                    }
                }
                Err(error) if error.to_string().contains("procedure \"Sequence\"") => {
                    found += 1;
                    failures.push(format!("{} {:?}: {error}", path.display(), form.form));
                }
                Ok(None) | Err(_) => {}
            }
        }
    }
    assert_eq!(found, 72, "observed Sequence count changed");
    assert!(
        failures.is_empty(),
        "Sequence entries must decode: {failures:?}"
    );
}
