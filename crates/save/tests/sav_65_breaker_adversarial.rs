// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::CreatedObjects;

fn vsval_wide(value: u32) -> [u8; 2] {
    assert!(value <= 0x3fff);
    (((value << 2) | 1) as u16).to_le_bytes()
}

fn object(form: [u8; 3], times_used: u32, effects: &[([u8; 3], u32, u32, u32)]) -> Vec<u8> {
    let mut bytes = form.to_vec();
    bytes.extend(times_used.to_le_bytes());
    bytes.extend(vsval_wide(effects.len() as u32));
    for (effect, magnitude, duration, area) in effects {
        bytes.extend(effect);
        bytes.extend(magnitude.to_le_bytes());
        bytes.extend(duration.to_le_bytes());
        bytes.extend(area.to_le_bytes());
        bytes.extend((-0.0_f32).to_bits().to_le_bytes());
    }
    bytes
}

#[test]
fn sav_65_each_table_keeps_its_own_wide_count_and_ordered_effects() {
    let mut bytes = Vec::new();
    bytes.extend(vsval_wide(2));
    bytes.extend(object([0x40, 0x01, 0x02], 0x1122_3344, &[]));
    bytes.extend(object(
        [0x80, 0x03, 0x04],
        9,
        &[
            ([0x40, 0x05, 0x06], 0x7fc0_0123, 7, 8),
            ([0xc0, 0x09, 0x0a], 0x8000_0000, 11, 12),
        ],
    ));
    bytes.push(0);
    bytes.extend(vsval_wide(1));
    bytes.extend(object([0x40, 0x0b, 0x0c], 13, &[]));
    bytes.extend(vsval_wide(1));
    bytes.extend(object(
        [0x40, 0x0d, 0x0e],
        14,
        &[([0x40, 0x0f, 0x10], 0x3f80_0000, 15, 16)],
    ));

    let decoded = CreatedObjects::read(&bytes).unwrap();
    assert_eq!(decoded.weapon_enchantments.len(), 2);
    assert_eq!(decoded.weapon_enchantments[0].times_used, 0x1122_3344);
    assert!(decoded.weapon_enchantments[0].effects.is_empty());
    let effects = &decoded.weapon_enchantments[1].effects;
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].effect.raw(), 0x400506);
    assert_eq!(effects[0].magnitude.to_bits(), 0x7fc0_0123);
    assert_eq!((effects[0].duration, effects[0].area), (7, 8));
    assert_eq!(effects[1].effect.raw(), 0xc0090a);
    assert_eq!(effects[1].magnitude.to_bits(), 0x8000_0000);
    assert_eq!((effects[1].duration, effects[1].area), (11, 12));
    assert!(decoded.armor_enchantments.is_empty());
    assert_eq!(decoded.potions[0].form.raw(), 0x400b0c);
    assert_eq!(decoded.poisons[0].effects[0].effect.raw(), 0x400f10);
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
}

#[test]
fn sav_65_rejects_bad_counts_after_each_prior_table_without_partial_decode() {
    for bad_table in 0..4 {
        let mut bytes = Vec::new();
        for table in 0..4 {
            if table == bad_table {
                bytes.extend([0xfe, 0xff, 0xff, 0xff]);
                break;
            }
            bytes.push(0);
        }
        assert!(CreatedObjects::read(&bytes).is_err(), "table={bad_table}");
    }
}
