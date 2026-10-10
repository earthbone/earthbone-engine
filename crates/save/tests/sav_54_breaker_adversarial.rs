// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check the source-defined saved reference mask layout.

use soe_save::{reference::flags, ChangeForm, RefId};

fn moved_reference(payload: Vec<u8>) -> ChangeForm {
    let mut data = vec![0x40, 0x08, 0x00];
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        data.extend(value.to_le_bytes());
    }
    data.extend(payload);
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x0A, 0x00]),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 0,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_54_mask_words_are_distinct_little_endian_values_with_exact_suffix_boundary() {
    // xEdit defines u32 Mask Invert Flags then u16 Mask Set unk010.
    // FallrimTools reads and writes the same two values without applying them.
    for form_version in [77, 78] {
        for (mask, second) in [(0, 0), (u32::MAX, u16::MAX), (0x8000_0400, 0xA55A)] {
            let change = moved_reference(
                mask.to_le_bytes()
                    .into_iter()
                    .chain(second.to_le_bytes())
                    .chain([0xDE, 0xAD])
                    .collect(),
            );
            let header = change.reference_header(form_version).unwrap().unwrap();
            assert_eq!(header.form_flags, Some((mask, second)));
            assert_eq!(header.remaining_bytes, 2);
            assert_eq!(change.payload().unwrap()[header.bytes_consumed..], [0xDE, 0xAD]);
        }
    }
}

#[test]
fn sav_54_partial_mask_word_is_rejected_at_each_truncated_boundary() {
    for present in 0..6 {
        let payload = (0..present).map(|byte| byte as u8).collect();
        let change = moved_reference(payload);
        assert!(change.reference_header(78).is_err(), "accepted {present} mask bytes");
    }
}
