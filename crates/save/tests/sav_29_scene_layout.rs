// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCEN runtime boundaries from the external corpus, checked against xEdit's
//! plugin SCEN lists and CommonLib's action discriminators.

use soe_save::{
    dialogue_scene::{SceneActionKind, SceneRuntime},
    ChangeForm, Error, RefId, Save,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn form(data: Vec<u8>, flags: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: 37,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn runtime_bytes() -> Vec<u8> {
    let mut data = vec![1];
    data.extend(u32::MAX.to_le_bytes());
    data.extend([0xFE, 0xF0]);
    data.extend(3u32.to_le_bytes());
    for (kind, time, flags) in [
        (0u32, 0x8000_0000u32, [0xA5, 0xFF]),
        (1, 0x7FC0_0001, [0x80, 0xD1]),
        (2, 0x3F80_0000, [0x12, 0x04]),
    ] {
        data.extend(kind.to_le_bytes());
        data.extend(time.to_le_bytes());
        data.extend(flags);
        if kind == 2 {
            data.extend([0, 0x81]);
        }
    }
    data.extend([0xF2, 0x3C]);
    data.extend(0x7FA0_0003u32.to_le_bytes());
    data.extend(2u32.to_le_bytes());
    data.extend([0x10, 0x80, 0xFF, 0, 0xFE, 1]);
    data.extend(0x8000_0001u32.to_le_bytes());
    data
}

#[test]
fn sav_29_scene_runtime_retains_fields_widths_float_bits_and_order() {
    let data = runtime_bytes();
    let runtime = SceneRuntime::read(&data).unwrap();
    assert_eq!(
        (runtime.marker, runtime.phase, runtime.flags),
        (1, u32::MAX, [0xFE, 0xF0])
    );
    assert_eq!(runtime.actions.len(), 3);
    for (action, (kind, bits, flags)) in runtime.actions.iter().zip([
        (SceneActionKind::Dialogue, 0x8000_0000, [0xA5, 0xFF]),
        (SceneActionKind::Package, 0x7FC0_0001, [0x80, 0xD1]),
        (SceneActionKind::Timer, 0x3F80_0000, [0x12, 0x04]),
    ]) {
        assert_eq!(
            (action.kind, action.time_bits, action.flags),
            (kind, bits, flags)
        );
        assert_eq!(
            action.timer_flags,
            (kind == SceneActionKind::Timer).then_some([0, 0x81])
        );
    }
    assert_eq!(runtime.suffix_flags, [0xF2, 0x3C]);
    assert_eq!(runtime.time_bits, 0x7FA0_0003);
    assert_eq!(runtime.phases, [[0x10, 0x80, 0xFF], [0, 0xFE, 1]]);
    assert_eq!(runtime.trailing, 0x8000_0001);
    assert_eq!(runtime.data_bytes().unwrap(), data);
    let scene = form(data.clone(), 1 << 31).scene_change().unwrap().unwrap();
    assert_eq!(scene.runtime_data().unwrap(), Some(runtime));
    assert_eq!(scene.data_bytes().unwrap(), data);
}

#[test]
fn sav_29_scene_runtime_rejects_every_truncated_prefix_and_extra_byte() {
    let data = runtime_bytes();
    for flags in [1 << 31, (1 << 31) | 1] {
        let mut payload = if flags & 1 != 0 { vec![0; 6] } else { vec![] };
        payload.extend(&data);
        for cut in 0..payload.len() {
            assert!(
                form(payload[..cut].to_vec(), flags).scene_change().is_err(),
                "flags {flags:x}, cut {cut}"
            );
        }
        payload.push(0);
        assert!(matches!(
            form(payload, flags).scene_change(),
            Err(Error::DialogueScene { .. })
        ));
    }
    for cut in 0..data.len() {
        assert!(SceneRuntime::read(&data[..cut]).is_err(), "cut {cut}");
    }
}

#[test]
fn sav_29_scene_runtime_rejects_bad_counts_and_unknown_action_types() {
    for offset in [7, 49] {
        for count in [u32::MAX, 0, 1, 4] {
            let mut data = runtime_bytes();
            data[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
            assert!(
                form(data, 1 << 31).scene_change().is_err(),
                "offset {offset}, count {count}"
            );
        }
    }
    for kind in [3u32, 0x8000_0000, u32::MAX] {
        let mut data = runtime_bytes();
        data[11..15].copy_from_slice(&kind.to_le_bytes());
        assert!(matches!(
            SceneRuntime::read(&data),
            Err(Error::DialogueScene { .. })
        ));
    }
}

#[test]
fn sav_29_scene_runtime_empty_arrays_and_absent_marker_are_distinct() {
    let mut data = vec![0; 25];
    data[0] = 1;
    let present = form(data.clone(), 1 << 31).scene_change().unwrap().unwrap();
    let runtime = present.runtime_data().unwrap().unwrap();
    assert!(runtime.actions.is_empty());
    assert!(runtime.phases.is_empty());
    assert_eq!(runtime.data_bytes().unwrap(), data);
    let absent = form(vec![], 0).scene_change().unwrap().unwrap();
    assert_eq!(absent.runtime_data().unwrap(), None);
    assert!(form(vec![], 1 << 31).scene_change().is_err());
}

#[test]
fn sav_29_scene_runtime_encoder_rejects_wrong_timer_width_and_invalid_raw_data() {
    let runtime = SceneRuntime::read(&runtime_bytes()).unwrap();
    for index in 0..3 {
        let mut invalid = runtime.clone();
        invalid.actions[index].timer_flags = if index == 2 { None } else { Some([0, 1]) };
        assert!(invalid.data_bytes().is_err());
    }
    let mut scene = form(runtime_bytes(), 1 << 31)
        .scene_change()
        .unwrap()
        .unwrap();
    scene.active_data.pop();
    assert!(scene.runtime_data().is_err());
    assert!(scene.data_bytes().is_err());
}

#[test]
fn sav_29_scene_state_bits_never_gate_runtime_fields() {
    let original = SceneRuntime::read(&runtime_bytes()).unwrap();
    for bit in 0..8 {
        let byte = 1 << bit;
        let mut runtime = original.clone();
        runtime.marker = byte;
        runtime.flags = [byte; 2];
        runtime.suffix_flags = [byte; 2];
        for action in &mut runtime.actions {
            action.flags = [byte; 2];
            if let Some(flags) = &mut action.timer_flags {
                *flags = [byte; 2];
            }
        }
        runtime.phases.fill([byte; 3]);
        let data = runtime.data_bytes().unwrap();
        assert_eq!(data.len(), runtime_bytes().len());
        assert_eq!(SceneRuntime::read(&data).unwrap(), runtime);
    }
}

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

fn corpus_paths() -> Option<Vec<PathBuf>> {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SCEN corpus");
        return None;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 60, "expected all sixty corpus saves");
    Some(paths)
}

fn kind_number(kind: SceneActionKind) -> u16 {
    match kind {
        SceneActionKind::Dialogue => 0,
        SceneActionKind::Package => 1,
        SceneActionKind::Timer => 2,
    }
}

#[test]
fn sav_29_scene_corpus_exact_layout_truncation_and_consecutive_diffs() {
    let Some(paths) = corpus_paths() else { return };
    let mut snapshots = Vec::new();
    let mut variants = BTreeMap::new();
    let mut kinds = [0usize; 3];
    let (mut count, mut bytes, mut cuts) = (0usize, 0usize, 0usize);
    for path in paths {
        let save = Save::open(&path).unwrap();
        let mut snapshot = BTreeMap::new();
        for change in save
            .change_forms
            .iter()
            .filter(|form| form.record_type() == "SCEN")
        {
            let scene = change.scene_change().unwrap().unwrap();
            let runtime = scene.runtime_data().unwrap().unwrap();
            let encoded = runtime.data_bytes().unwrap();
            assert_eq!(
                encoded,
                scene.active_data,
                "{} {:?}",
                path.display(),
                change.form
            );
            assert_eq!(
                scene.data_bytes().unwrap(),
                change.payload().unwrap().as_ref()
            );
            assert_eq!(scene.change_flags(), change.change_flags);
            let timers = runtime
                .actions
                .iter()
                .filter(|a| a.kind == SceneActionKind::Timer)
                .count();
            assert_eq!(
                encoded.len(),
                25 + 10 * runtime.actions.len() + 2 * timers + 3 * runtime.phases.len()
            );
            for action in &runtime.actions {
                kinds[usize::from(kind_number(action.kind))] += 1;
            }
            for cut in 0..encoded.len() {
                assert!(
                    SceneRuntime::read(&encoded[..cut]).is_err(),
                    "{} {:?} cut {cut}",
                    path.display(),
                    change.form
                );
                cuts += 1;
            }
            let mut extra = encoded.clone();
            extra.push(0);
            assert!(SceneRuntime::read(&extra).is_err());
            *variants
                .entry((save.form_version, change.version, change.change_flags))
                .or_insert(0usize) += 1;
            snapshot.insert(save.resolve(change.form).unwrap(), runtime);
            count += 1;
            bytes += encoded.len();
        }
        snapshots.push((save.header.save_number, path, snapshot));
    }
    snapshots.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let (mut comparisons, mut changed) = (0usize, 0usize);
    for pair in snapshots.windows(2) {
        for (id, before) in &pair[0].2 {
            if let Some(after) = pair[1].2.get(id) {
                assert_eq!(
                    before.actions.iter().map(|a| a.kind).collect::<Vec<_>>(),
                    after.actions.iter().map(|a| a.kind).collect::<Vec<_>>(),
                    "scene {id:x}"
                );
                assert_eq!(before.phases.len(), after.phases.len(), "scene {id:x}");
                comparisons += 1;
                changed += usize::from(before != after);
            }
        }
    }
    eprintln!("SAV-29 SCEN {} saves, {count} forms, {bytes} bytes, {cuts} rejected cuts, kinds={kinds:?}, variants={variants:x?}, {comparisons} consecutive comparisons, {changed} changed", snapshots.len());
    assert!(count >= 807 && kinds.iter().all(|n| *n > 0));
    assert!(comparisons > 0 && changed > 0);
}

#[test]
fn sav_29_scene_corpus_matches_installed_vanilla_action_types_and_phase_counts() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping vanilla SCEN correlation");
        return;
    };
    let Some(paths) = corpus_paths() else { return };
    let plugin =
        soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap())
            .unwrap();
    let scenes: BTreeMap<_, _> = plugin
        .records
        .iter()
        .filter(|r| r.tag.0 == *b"SCEN")
        .map(|r| {
            (
                r.form_id,
                soe_esm::records::gameplay::Scene::from_record(r).unwrap(),
            )
        })
        .collect();
    let (mut matched, mut other) = (0usize, 0usize);
    for path in paths {
        let save = Save::open(path).unwrap();
        for form in save
            .change_forms
            .iter()
            .filter(|f| f.record_type() == "SCEN")
        {
            let runtime = form
                .scene_change()
                .unwrap()
                .unwrap()
                .runtime_data()
                .unwrap()
                .unwrap();
            let id = save.resolve(form.form).unwrap();
            let Some(scene) = scenes.get(&id) else {
                other += 1;
                continue;
            };
            assert_eq!(runtime.actions.len(), scene.actions.len(), "scene {id:x}");
            assert_eq!(runtime.phases.len(), scene.phases.len(), "scene {id:x}");
            for (saved, base) in runtime.actions.iter().zip(&scene.actions) {
                assert_eq!(kind_number(saved.kind), base.action_type, "scene {id:x}");
            }
            matched += 1;
        }
    }
    eprintln!("SAV-29 SCEN {matched} Skyrim.esm matches, {other} changes outside Skyrim.esm");
    assert!(matched >= 673);
}
