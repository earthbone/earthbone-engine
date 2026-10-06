// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-27 adversarial checks for supported CELL fields and opaque spatial tails.

use soe_save::{CellSeenData, ChangeForm, Error, RefId, Save};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const CORPUS: &str =
    "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

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

#[test]
fn sav_27_cell_seen_data_selects_exterior_and_repeated_interior_shapes() {
    let exterior = (0..32).collect::<Vec<u8>>();
    let decoded = form(6, 1 << 31, exterior.clone())
        .cell_change()
        .unwrap()
        .unwrap();
    assert_eq!(
        decoded.seen_data,
        Some(CellSeenData::Exterior(exterior.clone()))
    );
    assert_eq!(decoded.data_bytes(), exterior);

    let mut interior = vec![9, 0xff];
    interior.extend(0..32);
    interior.extend([7, 0x80]);
    interior.extend(0..32);
    let decoded = form(6, 1 << 31, interior.clone())
        .cell_change()
        .unwrap()
        .unwrap();
    assert!(
        matches!(decoded.seen_data, Some(CellSeenData::Interior(ref entries)) if entries.len() == 2 && entries[0].second == -1 && entries[1].second == -128)
    );
    assert_eq!(decoded.data_bytes(), interior);
}

#[test]
fn sav_27_cell_requires_flagged_prefix_and_suffix_fields_in_source_order() {
    let flags = 1 | 2 | 4 | 8;
    let mut bytes = vec![0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6];
    bytes.extend(0x1234_u16.to_le_bytes());
    bytes.extend(3_u16.to_le_bytes());
    bytes.extend(*b"Map");
    bytes.extend([0x40, 0x12, 0x34]);
    let decoded = form(6, flags, bytes.clone())
        .cell_change()
        .unwrap()
        .unwrap();
    assert_eq!(decoded.data_bytes(), bytes);
    assert!(form(6, flags, vec![1, 2, 3],).cell_change().is_err());
}

#[test]
fn sav_27_cell_accepts_placeholder_markers_and_rejects_unknown_flags() {
    for flag in [1 << 28, 1 << 29, 1 << 30] {
        let decoded = form(6, flag, vec![]).cell_change().unwrap().unwrap();
        assert!(decoded.exterior_short || decoded.exterior_char || decoded.detach_time);
        assert!(decoded.data_bytes().is_empty());
    }
    assert!(form(6, 1 << 4, vec![]).cell_change().is_err());
    assert!(form(6, 0, vec![0xaa]).cell_change().is_err());
    assert!(form(6, 4, vec![2, 0, b'x']).cell_change().is_err());
}

#[test]
fn sav_27_eczn_and_lctn_keep_unknown_payload_bytes_and_reject_short_common_flags() {
    for (kind, method) in [(29, true), (38, false)] {
        let flags = if method {
            1 | (1 << 31)
        } else {
            1 | (1 << 30) | (1 << 31)
        };
        let bytes = vec![1, 2, 3, 4, 5, 6, 0xaa, 0xbb];
        let value = form(kind, flags, bytes.clone());
        let decoded = if method {
            value.encounter_zone_change()
        } else {
            value.location_change()
        }
        .unwrap()
        .unwrap();
        assert_eq!(decoded.data_bytes(), bytes);
        for len in 0..6 {
            let bad = form(kind, 1, vec![0; len]);
            let result = if method {
                bad.encounter_zone_change()
            } else {
                bad.location_change()
            };
            assert!(result.is_err(), "accepted {kind} common flags length {len}");
        }
    }
}

#[test]
fn sav_27_spatial_decoders_decline_other_record_types() {
    let other = form(6, 0, vec![]);
    assert!(other.encounter_zone_change().unwrap().is_none());
    assert!(other.location_change().unwrap().is_none());
    let other = form(29, 0, vec![]);
    assert!(other.cell_change().unwrap().is_none());
}

#[test]
fn sav_27_external_corpus_accounts_for_all_spatial_records_and_raw_bytes() {
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    if paths.is_empty() {
        eprintln!("no external saves found, skipping SAV-27 corpus survey");
        return;
    }
    assert!(
        paths.len() >= 50,
        "SAV-27 survey requires the external corpus"
    );
    let mut totals = BTreeMap::from([
        ("CELL", (0_usize, 0_usize)),
        ("LCTN", (0, 0)),
        ("ECZN", (0, 0)),
    ]);
    let mut decoded_counts = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    let mut unsupported_counts = BTreeMap::from([("CELL", 0_usize), ("LCTN", 0), ("ECZN", 0)]);
    for path in paths {
        let save = Save::open(path).unwrap();
        for change in &save.change_forms {
            let name = change.record_type();
            let Some(total) = totals.get_mut(name) else {
                continue;
            };
            let payload = change.payload().unwrap();
            total.0 += 1;
            total.1 += payload.len();
            let result = match name {
                "CELL" => change
                    .cell_change()
                    .map(|value| value.map(|decoded| decoded.data_bytes())),
                "LCTN" => change
                    .location_change()
                    .map(|value| value.map(|decoded| decoded.data_bytes())),
                "ECZN" => change
                    .encounter_zone_change()
                    .map(|value| value.map(|decoded| decoded.data_bytes())),
                _ => unreachable!(),
            };
            match result {
                Ok(Some(bytes)) => {
                    assert_eq!(
                        bytes,
                        payload.as_ref(),
                        "{name} flags={:#010x}",
                        change.change_flags
                    );
                    *decoded_counts.get_mut(name).unwrap() += 1;
                }
                Err(Error::SpatialChange { .. }) => *unsupported_counts.get_mut(name).unwrap() += 1,
                Ok(None) => panic!("spatial decoder declined {name}"),
                Err(error) => panic!("{name} flags={:#010x}: {error}", change.change_flags),
            }
        }
    }
    for name in ["CELL", "LCTN", "ECZN"] {
        assert!(totals[name].0 > 0, "survey missed {name}");
    }
    eprintln!("SAV-27 external counts={totals:?} decoded={decoded_counts:?} unsupported={unsupported_counts:?}");
}
