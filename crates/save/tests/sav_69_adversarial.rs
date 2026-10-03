// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-69 checks for xEdit extra-list gating and retained lock fields.

use soe_save::{reference::Field, ChangeForm, RefId};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn change(type_field: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(0x400100),
        change_flags: flags,
        type_field,
        version: 0,
        data,
        uncompressed_len: 0,
    }
}

fn ownership_list() -> Vec<u8> {
    let mut data = vec![4, 33];
    reference(0x400811).write(&mut data);
    data
}

#[test]
fn sav_69_extra_list_presence_uses_refr_and_actor_change_flag_masks() {
    let reference_list = ownership_list();
    let refr = change(0, 1 << 6, reference_list.clone());
    assert_eq!(
        refr.reference_extra_data(78)
            .unwrap()
            .unwrap()
            .data_bytes()
            .unwrap(),
        reference_list
    );

    let no_refr_extra = change(0, 1 << 18, vec![0xFE, 0xED]);
    assert!(no_refr_extra.reference_extra_data(78).unwrap().is_none());

    let mut actor_data = vec![0xA5; 8];
    actor_data.extend(&reference_list);
    let actor = change(1, 1 << 18, actor_data);
    assert_eq!(
        actor
            .reference_extra_data(78)
            .unwrap()
            .unwrap()
            .data_bytes()
            .unwrap(),
        reference_list
    );

    let actor_without_extra = change(1, 1 << 10, vec![0xA5; 8]);
    assert!(actor_without_extra
        .reference_extra_data(78)
        .unwrap()
        .is_none());
}

#[test]
fn sav_69_lock_level_byte_and_unknown_lock_words_roundtrip_exactly() {
    for level in [0, 1, 25, 26, 50, 51, 75, 76, 254, 255] {
        let mut bytes = vec![4, 42, level, 5];
        reference(0xFE0001).write(&mut bytes);
        bytes.extend(0xAABB_CCDD_u32.to_le_bytes());
        bytes.extend(0x1234_5678_u32.to_le_bytes());
        let form = change(0, 1 << 31, bytes.clone());
        let extra = form.reference_extra_data(78).unwrap().unwrap();
        assert!(matches!(
            extra.entries[0].fields.as_slice(),
            [Field::Byte(found), Field::Byte(5), Field::Reference(_), Field::Word(0xAABB_CCDD), Field::Word(0x1234_5678)] if *found == level
        ));
        assert_eq!(extra.data_bytes().unwrap(), bytes);
    }
}

#[test]
fn sav_69_package_before_owner_rank_global_and_lock_keeps_inventory_boundary() {
    for tree in ["Sequence", "Stacked"] {
        let mut bytes = vec![20, 108, 18, 0];
        soe_save::WString::from_text(tree).write(&mut bytes);
        soe_save::WString::from_text("Wait").write(&mut bytes);
        bytes.extend([0xA5; 4]);
        bytes.extend(0xDEAD_BEEFu32.to_le_bytes());
        bytes.push(33);
        reference(0x400811).write(&mut bytes);
        bytes.push(35);
        bytes.extend((-7_i32).to_le_bytes());
        bytes.push(34);
        reference(0x400812).write(&mut bytes);
        bytes.extend([42, 255, 5]);
        reference(0xFE0001).write(&mut bytes);
        bytes.extend(0xAABB_CCDD_u32.to_le_bytes());
        bytes.extend(0x1234_5678_u32.to_le_bytes());
        let list_end = bytes.len();
        bytes.push(4);
        reference(0x400800).write(&mut bytes);
        bytes.extend((-3_i32).to_le_bytes());
        bytes.push(0);

        let form = change(0, (1 << 31) | (1 << 5), bytes.clone());
        let contents = form.reference_contents(78).unwrap().unwrap();
        let extra = contents.extra.unwrap();
        assert_eq!(
            extra
                .entries
                .iter()
                .map(|entry| entry.kind)
                .collect::<Vec<_>>(),
            [108, 33, 35, 34, 42]
        );
        assert!(matches!(
            extra.entries[2].fields.as_slice(),
            [Field::Word(rank)] if *rank as i32 == -7
        ));
        assert!(matches!(
            extra.entries[3].fields.as_slice(),
            [Field::Reference(owner)] if *owner == reference(0x400812)
        ));
        assert_eq!(extra.data_bytes().unwrap(), bytes[..list_end]);
        assert_eq!(contents.end, bytes.len());
        assert_eq!(contents.remaining_bytes, 0);
    }
}

#[test]
fn sav_69_truncated_or_unknown_extra_layouts_fail_without_guessing_a_boundary() {
    let valid = change(0, 1 << 31, ownership_list());
    for cut in 0..valid.data.len() {
        let mut truncated = valid.clone();
        truncated.data.truncate(cut);
        assert!(truncated.reference_extra_data(78).is_err(), "cut {cut}");
    }

    let unknown = change(0, 1 << 31, vec![4, 250]);
    assert!(unknown.reference_extra_data(78).is_err());

    let malformed_lock = change(0, 1 << 31, vec![4, 42, 75, 1, 0]);
    assert!(malformed_lock.reference_extra_data(78).is_err());
}
