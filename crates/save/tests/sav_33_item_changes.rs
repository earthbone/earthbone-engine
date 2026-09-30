// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_esm::{LoadOrder, Plugin, Tag};
use soe_save::base_object::flags;
use soe_save::{ChangeForm, RefId};

fn form(record_type: u8, change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0, 1, 0x23]),
        change_flags,
        type_field: record_type,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_33_all_item_types_and_named_flag_combinations_reencode_exactly() {
    for record_type in [12, 23, 24] {
        for combination in 0..8 {
            let mut data = Vec::new();
            if combination & 1 != 0 {
                data.extend([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
            }
            data.extend([0xDE, 0xAD]);
            let change = form(record_type, combination, data.clone());
            let decoded = change.item_change().unwrap().unwrap();

            assert_eq!(decoded.record_type, change.record_type());
            assert_eq!(decoded.form_flags.is_some(), combination & 1 != 0);
            assert_eq!(decoded.value_changed, combination & 2 != 0);
            assert_eq!(decoded.full_name_changed, combination & 4 != 0);
            assert_eq!(decoded.change_flags(), combination);
            assert_eq!(decoded.data_bytes(), data);
        }
    }
}

#[test]
fn sav_33_unnamed_flags_and_tail_bytes_are_retained() {
    let flags = flags::FORM | flags::VALUE | 0x8000_0020;
    let data = vec![1, 2, 3, 4, 5, 6, 0xA5, 0x5A];
    let decoded = form(23, flags, data.clone())
        .item_change()
        .unwrap()
        .unwrap();

    assert_eq!(decoded.unnamed_flags, 0x8000_0020);
    assert_eq!(decoded.change_flags(), flags);
    assert_eq!(decoded.data_bytes(), data);
}

#[test]
fn sav_33_truncated_form_flags_are_rejected() {
    for len in 0..6 {
        let error = form(12, flags::FORM, vec![0; len])
            .item_change()
            .unwrap_err();
        assert!(matches!(error, soe_save::Error::Truncated { .. }));
    }
}

#[test]
fn sav_33_non_item_record_types_have_no_item_change() {
    assert_eq!(form(10, 0, vec![]).item_change().unwrap(), None);
}

#[test]
fn sav_33_installed_skyrim_contains_item_record_types() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed item record survey");
        return;
    };
    let mut order = LoadOrder::new();
    order
        .add(
            "Skyrim.esm",
            Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap(),
        )
        .unwrap();
    for tag in [*b"ARMO", *b"WEAP", *b"AMMO"] {
        assert!(
            order.file(0).plugin.records_of(Tag(tag)).next().is_some(),
            "Skyrim.esm should contain {:?}",
            std::str::from_utf8(&tag).unwrap()
        );
    }
}
