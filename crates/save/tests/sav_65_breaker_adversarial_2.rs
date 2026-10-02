// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{CreatedObjects, RefKind};

fn object(form: [u8; 3], times_used: u32, effect: [u8; 3], price: f32) -> Vec<u8> {
    let mut bytes = form.to_vec();
    bytes.extend(times_used.to_le_bytes());
    bytes.push(4);
    bytes.extend(effect);
    bytes.extend((-0.0_f32).to_le_bytes());
    bytes.extend(0x1122_3344_u32.to_le_bytes());
    bytes.extend(0x5566_7788_u32.to_le_bytes());
    bytes.extend(price.to_bits().to_le_bytes());
    bytes
}

#[test]
fn sav_65_invalid_refid_kind_and_nonfinite_float_bits_remain_verbatim() {
    let mut bytes = vec![4];
    bytes.extend(object(
        [0xC0, 0x12, 0x34],
        u32::MAX,
        [0xFF, 0xFE, 0xDC],
        f32::from_bits(0x7FC0_0123),
    ));
    bytes.extend([0, 0, 0]);
    let decoded = CreatedObjects::read(&bytes).unwrap();
    let entry = &decoded.weapon_enchantments[0];
    assert_eq!(entry.form.kind(), RefKind::Invalid);
    assert_eq!(entry.form.raw(), 0xC01234);
    assert_eq!(entry.times_used, u32::MAX);
    assert_eq!(entry.effects[0].effect.kind(), RefKind::Invalid);
    assert_eq!(entry.effects[0].effect.raw(), 0xFFFE_DC);
    assert_eq!(entry.effects[0].magnitude.to_bits(), 0x8000_0000);
    assert_eq!(entry.effects[0].price.to_bits(), 0x7FC0_0123);
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
}

#[test]
fn sav_65_four_table_sequence_keeps_each_table_boundary_and_field_order() {
    let mut bytes = vec![4];
    bytes.extend(object([0x40, 0, 1], 10, [0x40, 0, 2], 1.0));
    bytes.push(4);
    bytes.extend(object([0x40, 0, 3], 20, [0x40, 0, 4], 2.0));
    bytes.push(4);
    bytes.extend(object([0x40, 0, 5], 30, [0x40, 0, 6], 3.0));
    bytes.push(4);
    bytes.extend(object([0x40, 0, 7], 40, [0x40, 0, 8], 4.0));

    let decoded = CreatedObjects::read(&bytes).unwrap();
    for (table, form, effect, times, price) in [
        (&decoded.weapon_enchantments, 1, 2, 10, 1.0),
        (&decoded.armor_enchantments, 3, 4, 20, 2.0),
        (&decoded.potions, 5, 6, 30, 3.0),
        (&decoded.poisons, 7, 8, 40, 4.0),
    ] {
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].form.raw(), 0x400000 | form);
        assert_eq!(table[0].times_used, times);
        assert_eq!(table[0].effects[0].effect.raw(), 0x400000 | effect);
        assert_eq!(
            table[0].effects[0].magnitude.to_bits(),
            (-0.0_f32).to_bits()
        );
        assert_eq!(table[0].effects[0].duration, 0x1122_3344);
        assert_eq!(table[0].effects[0].area, 0x5566_7788);
        assert_eq!(table[0].effects[0].price, price);
    }
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
}
