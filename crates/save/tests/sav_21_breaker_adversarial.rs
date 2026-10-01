// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for saved Stacked package state.

use soe_save::{ChangeForm, RefId, Save, WString};
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

fn stacked_form() -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0, 0, 1]),
        change_flags: 1 << 11,
        type_field: 0,
        version: 0,
        uncompressed_len: 0,
        data: vec![4, 108, 18, 0],
    }
}

#[test]
fn every_observed_stacked_entry_roundtrips_and_keeps_its_following_boundary() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping Stacked corpus check");
        return;
    };
    let root = Path::new(&root);
    let mut paths = vec![];
    collect(root, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 60, "requires all observed saves");

    let mut found = 0;
    let mut errors = vec![];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in &save.change_forms {
            match form.reference_contents(save.form_version) {
                Ok(Some(contents)) => {
                    let bytes = contents.data_bytes().unwrap();
                    let payload = form.payload().unwrap();
                    assert_eq!(bytes, payload[contents.start..contents.end]);
                    assert_eq!(contents.start + bytes.len(), contents.end);
                    assert_eq!(contents.remaining_bytes, payload.len() - contents.end);
                }
                Err(error) if error.to_string().contains("Stacked") => {
                    found += 1;
                    errors.push(error.to_string());
                }
                Ok(None) | Err(_) => {}
            }
        }
    }
    assert_eq!(found, 9, "observed Stacked count changed");
    assert!(
        errors.is_empty(),
        "Stacked payloads must decode: {errors:?}"
    );
}

#[test]
fn opaque_stacked_body_is_not_accepted_using_a_later_terminator() {
    let mut change = stacked_form();
    WString::from_text("Stacked").write(&mut change.data);
    // The 255 terminator rule applies to PackageData's outer discriminator.
    // It does not define the end of this undocumented Stacked body.
    change.data.extend([0x11, 0x22, 108, 255]);

    let error = change.reference_contents(78).unwrap_err().to_string();
    assert!(error.contains("Stacked"), "{error}");
}

#[test]
fn truncated_stacked_name_is_rejected_without_partial_decode() {
    let mut change = stacked_form();
    WString::from_text("Stacked").write(&mut change.data);
    change.data.pop();

    assert!(change.reference_contents(78).is_err());
}
