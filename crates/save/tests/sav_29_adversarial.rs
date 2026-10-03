// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for INFO, SMQN and SCEN saved changes.

use soe_save::{dialogue_scene::SceneActionKind, ChangeForm, RefId, Save};
use std::{collections::BTreeMap, path::Path};

const FORM_FLAGS: u32 = 1;
const STATE: u32 = 1 << 31;

fn change(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn empty_scene() -> Vec<u8> {
    let mut data = vec![1];
    data.extend(0u32.to_le_bytes());
    data.extend([0xFF, 0x80]);
    data.extend(0u32.to_le_bytes());
    data.extend([0xFE, 0x01]);
    data.extend(0x7FC0_0001u32.to_le_bytes());
    data.extend(0u32.to_le_bytes());
    data.extend(0x8000_0001u32.to_le_bytes());
    data
}

fn mixed_scene() -> Vec<u8> {
    let mut data = empty_scene();
    data.splice(7..11, 3u32.to_le_bytes());
    for (index, kind) in [0u32, 1, 2].into_iter().enumerate() {
        let mut action = Vec::new();
        action.extend(kind.to_le_bytes());
        action.extend((0x7FC0_0000 + index as u32).to_le_bytes());
        action.extend([index as u8, 0xFF]);
        if kind == 2 {
            action.extend([0x80, 0x01]);
        }
        let offset = 11 + [0usize, 10, 20][index];
        data.splice(offset..offset, action);
    }
    data
}

#[test]
fn sav_29_adversarial_outer_flags_keep_typed_values_and_exact_payloads() {
    for (kind, state_data) in [
        (7, Vec::new()),
        (36, 0u32.to_le_bytes().to_vec()),
        (37, empty_scene()),
    ] {
        for flags in [0, FORM_FLAGS, STATE, FORM_FLAGS | STATE] {
            let mut data = if flags & FORM_FLAGS != 0 {
                vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
            } else {
                Vec::new()
            };
            if flags & STATE != 0 {
                data.extend(&state_data);
            }
            let form = change(kind, flags, data.clone());
            match kind {
                7 => {
                    let decoded = form.info_change().unwrap().unwrap();
                    assert_eq!(decoded.said_once, flags & STATE != 0);
                    assert_eq!(decoded.change_flags(), flags);
                    assert_eq!(decoded.data_bytes(), data);
                }
                36 => {
                    let decoded = form.story_manager_quest_node_change().unwrap().unwrap();
                    assert_eq!(decoded.last_run_times.is_some(), flags & STATE != 0);
                    assert_eq!(decoded.change_flags(), flags);
                    assert_eq!(decoded.data_bytes().unwrap(), data);
                }
                37 => {
                    let decoded = form.scene_change().unwrap().unwrap();
                    assert_eq!(decoded.active, flags & STATE != 0);
                    assert_eq!(decoded.change_flags(), flags);
                    assert_eq!(decoded.data_bytes().unwrap(), data);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn sav_29_adversarial_scene_action_types_have_exact_counted_widths() {
    let data = mixed_scene();
    let scene = change(37, STATE, data.clone())
        .scene_change()
        .unwrap()
        .unwrap();
    let runtime = scene.runtime_data().unwrap().unwrap();
    assert_eq!(
        runtime
            .actions
            .iter()
            .map(|action| action.kind)
            .collect::<Vec<_>>(),
        [
            SceneActionKind::Dialogue,
            SceneActionKind::Package,
            SceneActionKind::Timer
        ]
    );
    assert_eq!(runtime.actions[2].timer_flags, Some([0x80, 0x01]));
    assert_eq!(runtime.data_bytes().unwrap(), data);
    assert_eq!(scene.data_bytes().unwrap(), data);
}

#[test]
fn sav_29_adversarial_counts_reject_minimum_shortfalls_without_panicking() {
    let valid = mixed_scene();
    for count_offset in [7, 49] {
        for count in [u32::MAX, u32::MAX - 1, 0x4000_0000] {
            let mut data = valid.clone();
            data[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
            assert!(change(37, STATE, data).scene_change().is_err());
        }
    }
    for cut in 0..valid.len() {
        assert!(
            change(37, STATE, valid[..cut].to_vec())
                .scene_change()
                .is_err(),
            "truncated SCEN payload at {cut}"
        );
    }
}

#[test]
fn sav_29_adversarial_corpus_reencodes_all_scene_runtime_bytes() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-29 SCEN corpus check");
        return;
    };
    let mut paths = Vec::new();
    let mut pending = vec![Path::new(&root).to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    assert!(paths.len() >= 60);
    let (mut scenes, mut bytes) = (0usize, 0usize);
    let mut flag_variants = BTreeMap::new();
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in save
            .change_forms
            .iter()
            .filter(|form| form.record_type() == "SCEN")
        {
            let scene = form.scene_change().unwrap().unwrap();
            let encoded = scene.data_bytes().unwrap();
            assert_eq!(
                encoded,
                form.payload().unwrap().as_ref(),
                "{} {:?}",
                path.display(),
                form.form
            );
            assert_eq!(scene.change_flags(), form.change_flags);
            *flag_variants.entry(form.change_flags).or_insert(0usize) += 1;
            scenes += 1;
            bytes += encoded.len();
        }
    }
    eprintln!(
        "SAV-29 adversarial corpus: {scenes} SCEN changes, {bytes} bytes, flags={flag_variants:?}"
    );
    assert!(scenes >= 807);
    assert!(bytes > 0);
}

#[test]
fn sav_29_adversarial_corpus_actions_match_installed_scene_records() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-29 installed correlation");
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping SAV-29 installed correlation");
        return;
    };
    let plugin =
        soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap())
            .unwrap();
    let scenes: BTreeMap<_, _> = plugin
        .records
        .iter()
        .filter(|record| record.tag.0 == *b"SCEN")
        .map(|record| {
            (
                record.form_id,
                soe_esm::records::gameplay::Scene::from_record(record).unwrap(),
            )
        })
        .collect();
    let mut paths = Vec::new();
    let mut pending = vec![Path::new(&root).to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ess"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let mut matched = 0usize;
    for path in paths {
        let save = Save::open(path).unwrap();
        for form in save
            .change_forms
            .iter()
            .filter(|form| form.record_type() == "SCEN")
        {
            let scene = form.scene_change().unwrap().unwrap();
            let runtime = scene.runtime_data().unwrap().unwrap();
            let id = save.resolve(form.form).unwrap();
            let Some(base) = scenes.get(&id) else {
                continue;
            };
            assert_eq!(runtime.actions.len(), base.actions.len(), "scene {id:x}");
            assert_eq!(runtime.phases.len(), base.phases.len(), "scene {id:x}");
            for (saved, authored) in runtime.actions.iter().zip(&base.actions) {
                let kind = match saved.kind {
                    SceneActionKind::Dialogue => 0,
                    SceneActionKind::Package => 1,
                    SceneActionKind::Timer => 2,
                };
                assert_eq!(kind, authored.action_type, "scene {id:x}");
            }
            matched += 1;
        }
    }
    assert!(matched >= 673, "only {matched} installed scenes matched");
}
