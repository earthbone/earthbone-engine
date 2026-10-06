// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based adversarial checks for SAV-27 spatial change payloads.

use soe_save::{ChangeForm, Error, RefId, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn collect(path: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, paths);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn sav_27_cell_marker_flag_combinations_preserve_zero_width_markers_and_bytes() {
    for subset in 0..8_u32 {
        let flags =
            ((subset & 1) << 28) | (((subset >> 1) & 1) << 29) | (((subset >> 2) & 1) << 30);
        let bytes: &[u8] = if flags == 0 {
            &[]
        } else {
            &[0x11, 0x22, 0x33, 0x44]
        };
        let decoded = form(6, flags, bytes.to_vec())
            .cell_change()
            .unwrap()
            .unwrap();
        assert_eq!(decoded.exterior_short, flags & (1 << 28) != 0);
        assert_eq!(decoded.exterior_char, flags & (1 << 29) != 0);
        assert_eq!(decoded.detach_time, flags & (1 << 30) != 0);
        assert_eq!(decoded.opaque_suffix, bytes);
        assert_eq!(decoded.data_bytes(), bytes);
    }
}

#[test]
fn sav_27_cell_all_source_defined_flags_decode_in_source_order_and_round_trip() {
    let source_bits = [0_u8, 1, 2, 3, 28, 29, 30, 31];
    for subset in 0..1_usize << source_bits.len() {
        let flags = source_bits
            .iter()
            .enumerate()
            .filter(|(index, _)| subset & (1 << index) != 0)
            .fold(0_u32, |flags, (_, bit)| flags | (1 << bit));
        let mut bytes = Vec::new();
        if flags & 1 != 0 {
            bytes.extend([1, 2, 3, 4, 5, 6]);
        }
        if flags & 2 != 0 {
            bytes.extend([0x34, 0x12]);
        }
        if flags & (1 << 31) != 0 {
            bytes.extend(0_u8..32);
        }
        if flags & 4 != 0 {
            bytes.extend([2, 0, b'O', b'K']);
        }
        if flags & 8 != 0 {
            bytes.extend([0x40, 0x12, 0x34]);
        }
        if flags & (0b111 << 28) != 0 && flags & (1 << 31) == 0 {
            bytes.extend([0xaa, 0xbb, 0xcc]);
        }
        let decoded = form(6, flags, bytes.clone())
            .cell_change()
            .unwrap()
            .unwrap();
        assert_eq!(decoded.form_flags.is_some(), flags & 1 != 0);
        assert_eq!(decoded.cell_flags.is_some(), flags & 2 != 0);
        assert_eq!(decoded.full_name.is_some(), flags & 4 != 0);
        assert_eq!(decoded.owner.is_some(), flags & 8 != 0);
        assert_eq!(decoded.seen_data.is_some(), flags & (1 << 31) != 0);
        assert_eq!(decoded.data_bytes(), bytes, "flags={flags:#010x}");
    }
}

#[test]
fn sav_27_cell_rejects_truncated_seen_data_even_with_zero_width_markers() {
    for marker in [0, 1 << 28, 1 << 29, 1 << 30, 0x7000_0000] {
        for len in 1..32 {
            let result = form(6, (1 << 31) | marker, vec![0x55; len]).cell_change();
            assert!(
                result.is_err(),
                "accepted {len} seen-data bytes with marker flags {marker:#010x}"
            );
        }
    }
}

#[test]
fn sav_27_spatial_known_width_truncation_and_reserved_flags_reject() {
    for kind in [29, 38] {
        for len in 0..6 {
            let result = if kind == 29 {
                form(kind, 1, vec![0; len]).encounter_zone_change()
            } else {
                form(kind, 1, vec![0; len]).location_change()
            };
            assert!(
                result.is_err(),
                "accepted {len} common flag bytes for {kind}"
            );
        }
    }
    assert!(form(29, 1 << 2, vec![]).encounter_zone_change().is_err());
    assert!(form(38, 1 << 29, vec![]).location_change().is_err());
}

#[test]
fn sav_27_external_save_corpus_accounts_for_spatial_records_and_bytes() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping SAV-27 save census");
        return;
    };
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50, "expected the 60-save external corpus");

    let mut totals = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut raw_bytes = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut decoded = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut unsupported = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    for path in paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let name = change.record_type();
            if !totals.contains_key(name) {
                continue;
            }
            let payload = change.payload().unwrap();
            *totals.get_mut(name).unwrap() += 1;
            *raw_bytes.get_mut(name).unwrap() += payload.len();
            let result = match name {
                "CELL" => change.cell_change().map(|v| v.map(|c| c.data_bytes())),
                "LCTN" => change.location_change().map(|v| v.map(|c| c.data_bytes())),
                "ECZN" => change
                    .encounter_zone_change()
                    .map(|v| v.map(|c| c.data_bytes())),
                _ => unreachable!(),
            };
            match result {
                Ok(Some(bytes)) => {
                    assert_eq!(
                        bytes,
                        payload.as_ref(),
                        "{name} {:#010x}",
                        change.change_flags
                    );
                    *decoded.get_mut(name).unwrap() += 1;
                }
                Err(Error::SpatialChange { .. }) => *unsupported.get_mut(name).unwrap() += 1,
                other => panic!("unaccounted {name} result: {other:?}"),
            }
        }
    }
    for name in ["CELL", "LCTN", "ECZN"] {
        assert!(totals[name] > 0, "corpus contains no {name} records");
        assert_eq!(decoded[name] + unsupported[name], totals[name]);
    }
    eprintln!("SAV-27 corpus records={totals:?} bytes={raw_bytes:?} decoded={decoded:?} unsupported={unsupported:?}");
}
