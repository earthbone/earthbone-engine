// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! INFO, SMQN and SCEN change fixtures from xEdit save definitions, with a
//! corpus check over external saves. FallrimTools retains these types raw.

use std::{collections::BTreeMap, io::Write, path::Path};

use soe_save::{ChangeForm, RefId, Save};

const TYPES: [u8; 3] = [7, 36, 37];
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

fn decoded(form: &ChangeForm) -> soe_save::Result<(u32, Vec<u8>)> {
    match form.record_type() {
        "INFO" => {
            let change = form.info_change()?.unwrap();
            Ok((change.change_flags(), change.data_bytes()))
        }
        "SMQN" => {
            let change = form.story_manager_quest_node_change()?.unwrap();
            Ok((change.change_flags(), change.data_bytes()?))
        }
        "SCEN" => {
            let change = form.scene_change()?.unwrap();
            Ok((change.change_flags(), change.data_bytes()?))
        }
        _ => unreachable!(),
    }
}

#[test]
fn sav_29_all_types_preserve_each_source_flag_combination() {
    for kind in TYPES {
        for flags in [0, 1, STATE, STATE | 1] {
            let mut data = if flags & 1 != 0 {
                vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6]
            } else {
                vec![]
            };
            if flags & STATE != 0 {
                if kind == 36 {
                    data.extend(2u32.to_le_bytes());
                    data.extend(0x8000_0000u32.to_le_bytes());
                    data.extend(0x7FC0_0001u32.to_le_bytes());
                } else if kind == 37 {
                    data.extend([0x01, 0xFF, 0x00, 0xA5]);
                }
            }
            let form = change(kind, flags, data.clone());
            assert_eq!(decoded(&form).unwrap(), (flags, data), "type {kind}");
        }
    }
}

#[test]
fn sav_29_named_fields_retain_high_bits_and_state_markers() {
    let data = vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
    let info = change(7, 1 | STATE, data.clone())
        .info_change()
        .unwrap()
        .unwrap();
    assert_eq!(info.form_flags.unwrap().flags, 0xB4A39281);
    assert_eq!(info.form_flags.unwrap().unknown, 0xD6C5);
    assert!(info.said_once);

    let node = change(36, STATE, vec![1, 0, 0, 0, 0x80, 0, 0, 0])
        .story_manager_quest_node_change()
        .unwrap()
        .unwrap();
    assert_eq!(node.last_run_times, Some(vec![0x0000_0080]));
    assert_eq!(node.form_flags, None);

    let scene = change(37, STATE, vec![0x01, 0xFF, 0xA5])
        .scene_change()
        .unwrap()
        .unwrap();
    assert!(scene.active);
    assert_eq!(scene.active_data, [0x01, 0xFF, 0xA5]);
    assert_eq!(scene.form_flags, None);
}

#[test]
fn sav_29_rejects_truncated_defined_fields_and_unowned_bytes() {
    for kind in TYPES {
        for flags in [1, STATE | 1] {
            let mut data = vec![1, 2, 3, 4, 5, 6];
            if kind == 36 && flags & STATE != 0 {
                data.extend(2u32.to_le_bytes());
                data.extend(0x3F80_0000u32.to_le_bytes());
                data.extend(0x8000_0000u32.to_le_bytes());
            }
            let required = if kind == 37 { 6 } else { data.len() };
            for cut in 0..required {
                assert!(
                    decoded(&change(kind, flags, data[..cut].to_vec())).is_err(),
                    "type {kind}, flags {flags:#x}, cut {cut}"
                );
            }
        }
        for flags in [0, 1, STATE, STATE | 1] {
            let mut data = if flags & 1 != 0 { vec![0; 6] } else { vec![] };
            if kind == 36 && flags & STATE != 0 {
                data.extend(0u32.to_le_bytes());
            }
            data.push(0);
            if kind != 37 || flags & STATE == 0 {
                assert!(decoded(&change(kind, flags, data)).is_err());
            }
        }
    }
}

#[test]
fn sav_29_rejects_unknown_bits_and_ignores_other_types() {
    for kind in TYPES {
        for bit in 1..31 {
            assert!(decoded(&change(kind, 1 << bit, vec![])).is_err());
        }
    }
    let other = change(8, u32::MAX, vec![255]);
    assert!(other.info_change().unwrap().is_none());
    assert!(other.story_manager_quest_node_change().unwrap().is_none());
    assert!(other.scene_change().unwrap().is_none());
}

#[test]
fn sav_29_smqn_count_bounds_and_empty_section_are_distinct() {
    let empty = change(36, STATE, 0u32.to_le_bytes().to_vec())
        .story_manager_quest_node_change()
        .unwrap()
        .unwrap();
    assert_eq!(empty.last_run_times, Some(Vec::new()));
    let omitted = change(36, 0, vec![])
        .story_manager_quest_node_change()
        .unwrap()
        .unwrap();
    assert_eq!(omitted.last_run_times, None);
    assert!(decoded(&change(36, STATE, u32::MAX.to_le_bytes().to_vec())).is_err());
}

#[test]
fn sav_29_inactive_scene_rejects_active_data_on_encode() {
    let mut scene = change(37, 0, vec![]).scene_change().unwrap().unwrap();
    scene.active_data.push(0xA5);
    assert!(scene.data_bytes().is_err());
}

#[test]
fn sav_29_compressed_payloads_decode_and_reencode_exactly() {
    for kind in TYPES {
        let mut data = vec![1, 2, 3, 4, 5, 6];
        if kind == 36 {
            data.extend(1u32.to_le_bytes());
            data.extend(0x7FC0_0001u32.to_le_bytes());
        } else if kind == 37 {
            data.extend([0x01, 0xFF, 0xA5]);
        }
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&data).unwrap();
        let mut form = change(kind, 1 | STATE, encoder.finish().unwrap());
        form.uncompressed_len = data.len() as u32;
        assert_eq!(decoded(&form).unwrap(), (1 | STATE, data));
    }
}

fn collect(path: &Path, paths: &mut Vec<std::path::PathBuf>) {
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

#[test]
fn sav_29_external_corpus_accounts_for_every_matching_change() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping INFO/SMQN/SCEN corpus");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected at least 50 external saves");

    let mut counts = BTreeMap::from([("INFO", 0usize), ("SMQN", 0), ("SCEN", 0)]);
    let mut variants = BTreeMap::new();
    let mut bytes = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        for form in &save.change_forms {
            let Some(count) = counts.get_mut(form.record_type()) else {
                continue;
            };
            let (flags, encoded) = decoded(form).unwrap_or_else(|error| {
                panic!(
                    "{} {} {:06x}: {error}",
                    path.display(),
                    form.record_type(),
                    form.form.raw()
                )
            });
            assert_eq!(flags, form.change_flags);
            assert_eq!(encoded, form.payload().unwrap().as_ref());
            *count += 1;
            bytes += encoded.len();
            *variants
                .entry((save.form_version, form.version, form.record_type(), flags))
                .or_insert(0usize) += 1;
        }
    }
    eprintln!(
        "SAV-29 {} saves, {bytes} bytes, counts={counts:?}, versions/types/flags={variants:x?}",
        paths.len()
    );
    assert!(counts.values().all(|count| *count > 0));
}
