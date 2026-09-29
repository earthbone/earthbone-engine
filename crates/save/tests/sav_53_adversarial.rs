// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{base_object::flags, ChangeForm, RefId};

fn form(change_flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0, 1, 0x23]),
        change_flags,
        type_field: 10,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_53_each_shared_flag_combination_preserves_the_six_byte_prefix_boundary() {
    for mask in 0..8 {
        let change_flags = u32::from(mask & 1 != 0) * flags::FORM
            | u32::from(mask & 2 != 0) * flags::VALUE
            | u32::from(mask & 4 != 0) * flags::FULL_NAME;
        let mut data = Vec::new();
        if mask & 1 != 0 {
            data.extend([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        }
        data.extend([0xE1, 0xE2, 0xE3]);
        let decoded = form(change_flags, data).base_object_fields().unwrap().unwrap();
        assert_eq!(decoded.value_changed, mask & 2 != 0, "mask={mask}");
        assert_eq!(decoded.full_name_changed, mask & 4 != 0, "mask={mask}");
        assert_eq!(decoded.bytes_consumed, if mask & 1 != 0 { 6 } else { 0 });
        assert_eq!(decoded.remaining_bytes, 3);
    }
}

#[test]
fn sav_53_every_truncated_form_flag_prefix_is_rejected() {
    for length in 0..6 {
        let error = form(flags::FORM | flags::VALUE | flags::FULL_NAME, vec![0; length])
            .base_object_fields()
            .unwrap_err();
        assert!(matches!(error, soe_save::Error::Truncated { .. }), "length={length}: {error}");
    }
}

#[test]
fn sav_53_non_base_records_do_not_consume_payload_as_base_fields() {
    let mut change = form(flags::FORM | flags::VALUE | flags::FULL_NAME, vec![1, 2, 3]);
    change.type_field = 16;
    assert_eq!(change.base_object_fields().unwrap(), None);
}
