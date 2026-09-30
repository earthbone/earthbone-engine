// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-27 synthetic coverage from xEdit's defined spatial change fields.

use soe_save::{CellSeenData, ChangeForm, RefId};

const FORM_FLAGS: [u8; 6] = [0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];

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

fn cell_data(flags: u32, interior: bool) -> Vec<u8> {
    let mut data = Vec::new();
    if flags & 1 != 0 {
        data.extend(FORM_FLAGS);
    }
    if flags & 2 != 0 {
        data.extend(0xA581u16.to_le_bytes());
    }
    if flags & (1 << 31) != 0 {
        if interior {
            data.extend([7, 0xF9]);
            data.extend(0..32);
        } else {
            data.extend(0..32);
        }
    }
    if flags & 4 != 0 {
        data.extend(5u16.to_le_bytes());
        data.extend(*b"Cell!");
    }
    if flags & 8 != 0 {
        data.extend([0x40, 0x12, 0x34]);
    }
    data
}

#[test]
fn sav_27_cell_all_source_defined_flag_combinations_round_trip() {
    let known = [0u8, 1, 2, 3, 31];
    for subset in 0..1 << known.len() {
        let flags = known.iter().enumerate().fold(0, |flags, (index, bit)| {
            flags
                | if subset & (1 << index) != 0 {
                    1u32 << *bit
                } else {
                    0
                }
        });
        let data = cell_data(flags, false);
        let decoded = form(6, flags, data.clone()).cell_change().unwrap().unwrap();
        assert_eq!(decoded.data_bytes(), data, "flags={flags:#010x}");
    }
}

#[test]
fn sav_27_cell_interior_seen_data_round_trips_and_rejects_truncation() {
    let flags = (1 << 31) | 1 | 2 | 4 | 8;
    let data = cell_data(flags, true);
    let change = form(6, flags, data.clone());
    let decoded = change.cell_change().unwrap().unwrap();
    assert!(matches!(decoded.seen_data, Some(CellSeenData::Interior(_))));
    assert_eq!(decoded.data_bytes(), data);

    let mut known_prefix = FORM_FLAGS.to_vec();
    known_prefix.extend(0xA581u16.to_le_bytes());
    let mut suffix = Vec::new();
    suffix.extend(5u16.to_le_bytes());
    suffix.extend(*b"Cell!");
    suffix.extend([0x40, 0x12, 0x34]);
    for tail_len in 1..34 {
        if tail_len == 32 {
            // xEdit treats exactly 32 remaining bytes as exterior data, so a
            // truncated interior entry of that length is ambiguous.
            continue;
        }
        let mut truncated = known_prefix.clone();
        truncated.extend(0..tail_len);
        truncated.extend(&suffix);
        assert!(
            form(6, flags, truncated).cell_change().is_err(),
            "accepted truncated interior data at tail length {tail_len}"
        );
    }
    assert!(
        form(6, 2, vec![1]).cell_change().is_err(),
        "accepted truncated CELL flags"
    );
    assert!(
        form(6, 4, vec![3, 0, b'a']).cell_change().is_err(),
        "accepted truncated CELL name"
    );
    assert!(
        form(6, 8, vec![0x40, 0x12]).cell_change().is_err(),
        "accepted truncated CELL owner"
    );
    for len in 0..6 {
        assert!(
            form(6, 1, FORM_FLAGS[..len].to_vec())
                .cell_change()
                .is_err(),
            "accepted truncated common flags at length {len}"
        );
    }
    for len in 0..3 {
        assert!(
            form(6, 8, vec![0x40, 0x12, 0x34][..len].to_vec())
                .cell_change()
                .is_err(),
            "accepted truncated CELL owner at length {len}"
        );
    }
}

#[test]
fn sav_27_eczn_lctn_flag_combinations_keep_undefined_payload_bytes() {
    for (kind, known) in [(29, [0u8, 1, 31]), (38, [0u8, 30, 31])] {
        for subset in 0..1 << known.len() {
            let flags = known.iter().enumerate().fold(0, |flags, (index, bit)| {
                flags
                    | if subset & (1 << index) != 0 {
                        1u32 << *bit
                    } else {
                        0
                    }
            });
            let mut data = Vec::new();
            if flags & 1 != 0 {
                data.extend(FORM_FLAGS);
            }
            data.extend([0xDE, 0xAD, kind]);
            let decoded = match kind {
                29 => form(kind, flags, data.clone())
                    .encounter_zone_change()
                    .unwrap()
                    .unwrap(),
                _ => form(kind, flags, data.clone())
                    .location_change()
                    .unwrap()
                    .unwrap(),
            };
            assert_eq!(
                decoded.data_bytes(),
                data,
                "type={kind} flags={flags:#010x}"
            );
        }
    }
    for len in 0..6 {
        assert!(
            form(29, 1, FORM_FLAGS[..len].to_vec())
                .encounter_zone_change()
                .is_err(),
            "accepted truncated ECZN common form flags at length {len}"
        );
        assert!(
            form(38, 1, FORM_FLAGS[..len].to_vec())
                .location_change()
                .is_err(),
            "accepted truncated LCTN common form flags at length {len}"
        );
    }
}

#[test]
fn sav_27_cell_unlaid_high_flags_are_rejected() {
    for flags in [1 << 28, 1 << 29, 1 << 30, 0x6000_0000] {
        assert!(
            form(6, flags, vec![0; 8]).cell_change().is_err(),
            "accepted CELL flags {flags:#010x} without a source-defined layout"
        );
    }
}
