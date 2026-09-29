// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId, reference::flags};

fn change(version: u8, form_flags: Option<(u32, u16)>) -> ChangeForm {
    let data = form_flags.map_or_else(Vec::new, |(mask, unknown)| {
        [
            mask.to_le_bytes().as_slice(),
            unknown.to_le_bytes().as_slice(),
        ]
        .concat()
    });
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x0A, 0x00]),
        change_flags: if form_flags.is_some() { flags::FORM } else { 0 },
        type_field: 0,
        version,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_54_mask_words_are_retained_as_raw_data_across_reference_versions() {
    for version in [55, 78] {
        let original = change(version, Some((0x89AB_CDEF, 0xFEDC)));
        let header = original.reference_header(version).unwrap().unwrap();
        assert_eq!(header.form_flags, Some((0x89AB_CDEF, 0xFEDC)));
        assert_eq!(header.data_bytes().unwrap(), original.data);
    }
}

#[test]
fn sav_54_mask_presence_bit_controls_only_the_six_byte_payload_section() {
    let absent = change(78, None);
    let header = absent.reference_header(78).unwrap().unwrap();
    assert_eq!(header.form_flags, None);
    assert_eq!(header.bytes_consumed, 0);
    let present = change(78, Some((0, 0)));
    let header = present.reference_header(78).unwrap().unwrap();
    assert_eq!(header.form_flags, Some((0, 0)));
    assert_eq!(header.bytes_consumed, 6);
}

#[test]
fn sav_54_partial_mask_words_are_rejected_without_assigning_runtime_meaning() {
    for length in 0..6 {
        let mut partial = change(78, Some((0, 0)));
        partial.data.truncate(length);
        assert!(partial.reference_header(78).is_err(), "length={length}");
    }
}
