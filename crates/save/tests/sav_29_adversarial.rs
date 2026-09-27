// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_save::{ChangeForm, Save};

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
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

fn signature(form: &ChangeForm) -> (u32, u32, u8, Vec<u8>) {
    (
        form.form.raw(),
        form.change_flags,
        form.version,
        form.payload().unwrap().into_owned(),
    )
}

#[test]
fn sav_29_full_save_roundtrip_preserves_typed_change_payloads() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping INFO/SMQN/SCEN full-save roundtrip");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);

    let mut found = [0usize; 3];
    for path in paths {
        let save = Save::open(&path).unwrap();
        let expected: Vec<_> = save
            .change_forms
            .iter()
            .filter(|form| matches!(form.record_type(), "INFO" | "SMQN" | "SCEN"))
            .map(|form| {
                match form.record_type() {
                    "INFO" => assert!(form.info_change().unwrap().is_some()),
                    "SMQN" => assert!(form.story_manager_quest_node_change().unwrap().is_some()),
                    "SCEN" => assert!(form.scene_change().unwrap().is_some()),
                    _ => unreachable!(),
                }
                match form.record_type() {
                    "INFO" => found[0] += 1,
                    "SMQN" => found[1] += 1,
                    "SCEN" => found[2] += 1,
                    _ => unreachable!(),
                }
                signature(form)
            })
            .collect();
        let reopened = Save::parse(&save.write().unwrap()).unwrap();
        let actual: Vec<_> = reopened
            .change_forms
            .iter()
            .filter(|form| matches!(form.record_type(), "INFO" | "SMQN" | "SCEN"))
            .map(signature)
            .collect();
        assert_eq!(actual, expected, "{}", path.display());
    }
    assert!(found.iter().all(|count| *count > 0), "{found:?}");
}
