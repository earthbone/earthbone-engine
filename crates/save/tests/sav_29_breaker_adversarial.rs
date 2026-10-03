// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial boundary checks for SAV-29 dialogue and scene change forms.

use soe_save::{dialogue_scene::SceneActionKind, ChangeForm, RefId, Save, SceneRuntime};
use std::path::{Path, PathBuf};

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

fn scene(actions: &[(u32, u32)], phases: &[u8]) -> Vec<u8> {
    let mut data = vec![1];
    data.extend(u32::MAX.to_le_bytes());
    data.extend([0xFF, 0x80]);
    data.extend((actions.len() as u32).to_le_bytes());
    for (kind, time) in actions {
        data.extend(kind.to_le_bytes());
        data.extend(time.to_le_bytes());
        data.extend([0xA5, 0x80]);
        if *kind == 2 {
            data.extend([0xFF, 0x01]);
        }
    }
    data.extend([0x11, 0xFE]);
    data.extend(0x7FC0_0001u32.to_le_bytes());
    data.extend(((phases.len() / 3) as u32).to_le_bytes());
    data.extend(phases);
    data.extend(1u32.to_le_bytes());
    data
}

#[test]
fn sav_29_breaker_all_three_types_round_trip_each_known_flag_combination() {
    for kind in [7, 36, 37] {
        for flags in [0, FORM_FLAGS, STATE, FORM_FLAGS | STATE] {
            let mut payload = if flags & FORM_FLAGS != 0 {
                vec![0x80, 0xFF, 0x12, 0x34, 0xFE, 0xDC]
            } else {
                Vec::new()
            };
            if flags & STATE != 0 {
                match kind {
                    7 => {}
                    36 => {
                        payload.extend(2u32.to_le_bytes());
                        payload.extend(0x8000_0000u32.to_le_bytes());
                        payload.extend(0x7FC0_1234u32.to_le_bytes());
                    }
                    37 => payload.extend(scene(&[], &[])),
                    _ => unreachable!(),
                }
            }
            let form = change(kind, flags, payload.clone());
            match kind {
                7 => {
                    let decoded = form.info_change().unwrap().unwrap();
                    assert_eq!(decoded.said_once, flags & STATE != 0);
                    assert_eq!(
                        decoded.form_flags.map(|value| (value.flags, value.unknown)),
                        (flags & FORM_FLAGS != 0).then_some((0x3412_FF80, 0xDCFE))
                    );
                    assert_eq!(decoded.data_bytes(), payload);
                }
                36 => {
                    let decoded = form.story_manager_quest_node_change().unwrap().unwrap();
                    assert_eq!(
                        decoded.form_flags.map(|value| (value.flags, value.unknown)),
                        (flags & FORM_FLAGS != 0).then_some((0x3412_FF80, 0xDCFE))
                    );
                    assert_eq!(
                        decoded.last_run_times,
                        (flags & STATE != 0).then_some(vec![0x8000_0000, 0x7FC0_1234])
                    );
                    assert_eq!(decoded.data_bytes().unwrap(), payload);
                }
                37 => {
                    let decoded = form.scene_change().unwrap().unwrap();
                    assert_eq!(
                        decoded.form_flags.map(|value| (value.flags, value.unknown)),
                        (flags & FORM_FLAGS != 0).then_some((0x3412_FF80, 0xDCFE))
                    );
                    assert_eq!(decoded.data_bytes().unwrap(), payload);
                }
                _ => unreachable!(),
            }
            for cut in 0..payload.len() {
                let truncated = change(kind, flags, payload[..cut].to_vec());
                let rejected = match kind {
                    7 => truncated.info_change().is_err(),
                    36 => truncated.story_manager_quest_node_change().is_err(),
                    37 => truncated.scene_change().is_err(),
                    _ => unreachable!(),
                };
                assert!(rejected, "type {kind}, flags {flags:#x}, cut {cut}");
            }
        }
    }
}

#[test]
fn sav_29_breaker_scene_action_widths_follow_each_discriminator() {
    for (kind, expected, width) in [
        (0, SceneActionKind::Dialogue, 10),
        (1, SceneActionKind::Package, 10),
        (2, SceneActionKind::Timer, 12),
    ] {
        let bytes = scene(&[(kind, 0x8000_0000)], &[0x80, 0xFF, 0x01]);
        let decoded = SceneRuntime::read(&bytes).unwrap();
        assert_eq!(decoded.actions[0].kind, expected);
        assert_eq!(decoded.actions[0].time_bits, 0x8000_0000);
        assert_eq!(decoded.actions[0].timer_flags.is_some(), kind == 2);
        assert_eq!(bytes.len(), 25 + width + 3);
        assert_eq!(decoded.data_bytes().unwrap(), bytes);
    }
}

#[test]
fn sav_29_breaker_rejects_unknown_actions_count_overruns_and_truncation() {
    for bytes in [
        scene(&[(3, 0)], &[]),
        scene(&[(2, 0)], &[])[..scene(&[(2, 0)], &[]).len() - 2].to_vec(),
    ] {
        assert!(SceneRuntime::read(&bytes).is_err());
    }
    let valid = scene(&[(0, 0), (1, 0), (2, 0)], &[0x01, 0x02, 0x03]);
    for cut in 0..valid.len() {
        assert!(SceneRuntime::read(&valid[..cut]).is_err(), "cut {cut}");
    }
    for offset in [7, 49] {
        let mut malformed = valid.clone();
        malformed[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(SceneRuntime::read(&malformed).is_err());
    }
}

#[test]
fn sav_29_breaker_smqn_retains_empty_and_nonfinite_time_bit_patterns() {
    let cases: [Vec<u32>; 2] = [Vec::new(), vec![0x8000_0000, 0x7FC0_4321, 0x7FA0_0001]];
    for times in cases {
        let mut payload = vec![0x12, 0x34, 0x56, 0x78, 0xFE, 0xDC];
        payload.extend((times.len() as u32).to_le_bytes());
        for time in &times {
            payload.extend(time.to_le_bytes());
        }
        let form = change(36, FORM_FLAGS | STATE, payload.clone());
        let decoded = form.story_manager_quest_node_change().unwrap().unwrap();
        assert_eq!(decoded.last_run_times, Some(times));
        assert_eq!(decoded.data_bytes().unwrap(), payload);
    }
}

#[test]
fn sav_29_breaker_dispatch_returns_none_for_unrelated_record_types() {
    for kind in [6, 8, 35, 38, 63] {
        let form = change(kind, 0, vec![0xFF, 0xFF]);
        assert!(form.info_change().unwrap().is_none());
        assert!(form.story_manager_quest_node_change().unwrap().is_none());
        assert!(form.scene_change().unwrap().is_none());
    }
}

fn collect_saves(root: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, paths);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ess"))
        {
            paths.push(path);
        }
    }
}

#[test]
fn sav_29_breaker_external_corpus_accounts_for_all_three_types() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-29 corpus accounting");
        return;
    };
    let mut paths = Vec::new();
    collect_saves(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 60, "expected the sixty-save corpus");

    let mut totals = [0usize; 3];
    for path in paths {
        let save = Save::open(&path).unwrap();
        for form in save
            .change_forms
            .iter()
            .filter(|form| [7, 36, 37].contains(&form.type_index()))
        {
            let payload = form.payload().unwrap();
            match form.type_index() {
                7 => {
                    let decoded = form.info_change().unwrap().unwrap();
                    assert_eq!(decoded.data_bytes(), payload.as_ref());
                    totals[0] += 1;
                }
                36 => {
                    let decoded = form.story_manager_quest_node_change().unwrap().unwrap();
                    assert_eq!(decoded.data_bytes().unwrap(), payload.as_ref());
                    totals[1] += 1;
                }
                37 => {
                    let decoded = form.scene_change().unwrap().unwrap();
                    assert_eq!(decoded.data_bytes().unwrap(), payload.as_ref());
                    totals[2] += 1;
                }
                _ => unreachable!(),
            }
        }
    }
    assert_eq!(totals, [32_438, 3_054, 807]);
}
