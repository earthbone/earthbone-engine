// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-27 checks for saved spatial change payloads.

use soe_save::{CellSeenData, ChangeForm, Error, RefId, Save};
use std::path::{Path, PathBuf};

const COMMON_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 0x23]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn collect_ess(path: &Path, paths: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_ess(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn sav_27_cell_named_zero_width_markers_preserve_suffix_and_repeat() {
    for marker in 28..=30 {
        for base in [0, 1, 2, 3, 4, 8, 12] {
            let flags = base | (1 << marker);
            let mut data = Vec::new();
            if flags & 1 != 0 {
                data.extend(COMMON_FLAGS);
            }
            if flags & 2 != 0 {
                data.extend(0xA581u16.to_le_bytes());
            }
            if flags & 4 != 0 {
                data.extend(3u16.to_le_bytes());
                data.extend(*b"abc");
            }
            if flags & 8 != 0 {
                data.extend([0x40, 0x12, 0x34]);
            }
            let suffix = [0xF1, 0x00, marker];
            data.extend(suffix);
            let change = form(6, flags, data.clone());
            let first = change.cell_change().unwrap().unwrap();
            let second = change.cell_change().unwrap().unwrap();
            assert_eq!(first.exterior_short, marker == 28);
            assert_eq!(first.exterior_char, marker == 29);
            assert_eq!(first.detach_time, marker == 30);
            assert_eq!(first.opaque_suffix, suffix);
            assert_eq!(first.data_bytes(), data);
            assert_eq!(second, first);
        }
    }
}

#[test]
fn sav_27_cell_interior_entries_keep_signed_byte_and_reencode() {
    let flags = (1 << 31) | (1 << 28) | 1 | 2 | 4 | 8;
    let mut data = COMMON_FLAGS.to_vec();
    data.extend(0x1234u16.to_le_bytes());
    data.extend([0x07, 0xFF]);
    data.extend(0..32u8);
    data.extend([0x08, 0x80]);
    data.extend((32..64u8).rev());
    data.extend(3u16.to_le_bytes());
    data.extend(*b"abc");
    data.extend([0x40, 0x12, 0x34]);
    let change = form(6, flags, data.clone());
    let decoded = change.cell_change().unwrap().unwrap();
    let Some(CellSeenData::Interior(entries)) = &decoded.seen_data else {
        panic!("expected source-defined interior seen-data entries");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].first, 7);
    assert_eq!(entries[0].second, -1);
    assert_eq!(entries[1].first, 8);
    assert_eq!(entries[1].second, i8::MIN);
    assert_eq!(decoded.data_bytes(), data);
}

#[test]
fn sav_27_cell_rejects_unknown_flags_and_truncated_defined_fields() {
    for flags in [1 << 27, (1 << 28) | (1 << 27), 1 << 26] {
        assert!(form(6, flags, Vec::new()).cell_change().is_err());
    }
    assert!(form(6, 2, [0x81].to_vec()).cell_change().is_err());
    assert!(form(6, 4, [2, 0, b'a'].to_vec()).cell_change().is_err());
    assert!(form(6, 8, [0x40, 0x12].to_vec()).cell_change().is_err());
}

#[test]
fn sav_27_eczn_lctn_markers_are_zero_width_and_unknown_bits_reject() {
    for (kind, flags, first_marker, second_marker) in [
        (29, 0x8000_0002, true, true),
        (29, 0x8000_0001, false, true),
        (38, 0xC000_0001, true, true),
        (38, 0x4000_0001, true, false),
    ] {
        let mut data = Vec::new();
        if flags & 1 != 0 {
            data.extend(COMMON_FLAGS);
        }
        data.extend([0xA5, 0, kind, 0x5A]);
        let change = form(kind, flags, data.clone());
        let decoded = if kind == 29 {
            change.encounter_zone_change().unwrap().unwrap()
        } else {
            change.location_change().unwrap().unwrap()
        };
        if kind == 29 {
            assert_eq!(decoded.zone_flags_changed, first_marker);
            assert_eq!(decoded.game_data_changed, second_marker);
        } else {
            assert_eq!(decoded.keyword_data_changed, first_marker);
            assert_eq!(decoded.cleared_changed, second_marker);
        }
        assert_eq!(decoded.opaque_payload, [0xA5, 0, kind, 0x5A]);
        assert_eq!(decoded.data_bytes(), data);
    }
    assert!(form(29, 4, Vec::new()).encounter_zone_change().is_err());
    assert!(form(38, 1 << 29, Vec::new()).location_change().is_err());
    assert!(form(29, 1, COMMON_FLAGS[..5].to_vec())
        .encounter_zone_change()
        .is_err());
    assert!(form(38, 1, COMMON_FLAGS[..5].to_vec())
        .location_change()
        .is_err());
    assert!(form(6, 0, Vec::new())
        .encounter_zone_change()
        .unwrap()
        .is_none());
    assert!(form(6, 0, Vec::new()).location_change().unwrap().is_none());
}

#[test]
fn sav_27_corpus_accounts_for_every_spatial_form_and_round_trip() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping spatial form accounting");
        return;
    };
    let mut paths = Vec::new();
    collect_ess(Path::new(&root), &mut paths);
    paths.sort();
    assert!(
        paths.len() >= 50,
        "expected broad save corpus, got {}",
        paths.len()
    );

    let mut counts = [0usize; 3];
    let mut accounted = 0usize;
    for path in &paths {
        let save = Save::open(path).unwrap();
        let rewritten = Save::parse(&save.write().unwrap()).unwrap();
        assert_eq!(rewritten.change_forms.len(), save.change_forms.len());
        for (original, round_trip) in save.change_forms.iter().zip(&rewritten.change_forms) {
            let index = match original.record_type() {
                "CELL" => 0,
                "LCTN" => 1,
                "ECZN" => 2,
                _ => continue,
            };
            counts[index] += 1;
            accounted += 1;
            assert_eq!(round_trip.form, original.form);
            assert_eq!(round_trip.type_field, original.type_field);
            assert_eq!(round_trip.change_flags, original.change_flags);
            assert_eq!(round_trip.data, original.data);
            assert_eq!(round_trip.payload().unwrap(), original.payload().unwrap());
            let decoded = match index {
                0 => original
                    .cell_change()
                    .map(|value| value.map(|v| v.data_bytes())),
                1 => original
                    .location_change()
                    .map(|value| value.map(|v| v.data_bytes())),
                _ => original
                    .encounter_zone_change()
                    .map(|value| value.map(|v| v.data_bytes())),
            };
            match decoded {
                Ok(Some(bytes)) => {
                    assert_eq!(bytes.as_slice(), original.payload().unwrap().as_ref())
                }
                Err(Error::SpatialChange { .. }) => {
                    // Unknown or incomplete source layouts remain accounted raw data.
                }
                other => panic!("spatial record was not accounted by its decoder: {other:?}"),
            }
        }
    }
    assert!(
        counts.iter().all(|count| *count > 0),
        "no records for all types: {counts:?}"
    );
    assert_eq!(accounted, counts.iter().sum::<usize>());
}
