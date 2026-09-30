// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-53 base-object prefix boundaries from xEdit's Skyrim save definitions.

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
fn sav_53_value_flag_marks_a_zero_byte_field() {
    let change = form(10, flags::VALUE, vec![0xA5, 0x5A]);

    let fields = change.base_object_fields().unwrap().unwrap();

    assert!(fields.value_changed);
    assert!(!fields.full_name_changed);
    assert_eq!(fields.bytes_consumed, 0);
    assert_eq!(fields.remaining_bytes, 2);
}

#[test]
fn sav_53_full_name_flag_marks_a_zero_byte_field() {
    let change = form(10, flags::FULL_NAME, vec![0xA5, 0x5A]);

    let fields = change.base_object_fields().unwrap().unwrap();

    assert!(!fields.value_changed);
    assert!(fields.full_name_changed);
    assert_eq!(fields.bytes_consumed, 0);
    assert_eq!(fields.remaining_bytes, 2);
}

#[test]
fn sav_53_combined_flags_leave_suffix_at_exact_boundary() {
    let change = form(
        10,
        flags::FORM | flags::VALUE | flags::FULL_NAME,
        vec![1, 2, 3, 4, 5, 6, 0xA5, 0x5A],
    );

    let fields = change.base_object_fields().unwrap().unwrap();

    assert_eq!(fields.form_flags, Some((0x0403_0201, 0x0605)));
    assert!(fields.value_changed);
    assert!(fields.full_name_changed);
    assert_eq!(fields.bytes_consumed, 6);
    assert_eq!(fields.remaining_bytes, 2);
}

#[test]
fn sav_53_all_shared_types_use_the_same_field_boundary() {
    for record_type in [
        10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 33, 34, 35, 48,
    ] {
        let change = form(record_type, flags::VALUE | flags::FULL_NAME, vec![0xAB]);
        assert!(
            change.base_object_fields().unwrap().is_some(),
            "record type {record_type} should use the shared layout"
        );
    }
}

#[test]
fn sav_53_short_form_flags_report_truncation() {
    let change = form(10, flags::FORM, vec![1, 2, 3, 4, 5]);

    assert!(matches!(
        change.base_object_fields().unwrap_err(),
        soe_save::Error::Truncated {
            region: "base-object change",
            at: 4,
            need: 2,
            have: 1
        }
    ));
}

#[test]
fn sav_53_types_without_shared_fields_are_not_decoded() {
    let change = form(16, flags::VALUE | flags::FULL_NAME, vec![]);

    assert_eq!(change.base_object_fields().unwrap(), None);
}
