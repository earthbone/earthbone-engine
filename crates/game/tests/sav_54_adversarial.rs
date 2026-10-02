// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId, reference::flags};

fn moved_reference(payload: Vec<u8>, change_flags: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x0A, 0x00]),
        change_flags,
        type_field: 0,
        version: 78,
        uncompressed_len: 0,
        data: payload,
    }
}

fn placement_prefix() -> Vec<u8> {
    let mut payload = vec![0x40, 0x08, 0x00];
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        payload.extend(value.to_le_bytes());
    }
    payload
}

#[test]
fn sav_54_mask_field_reads_full_u32_then_u16_without_losing_unknown_bits() {
    for (mask, set_word) in [(0, 0), (u32::MAX, u16::MAX), (0x8000_0400, 0xA55A)] {
        let mut payload = placement_prefix();
        payload.extend(mask.to_le_bytes());
        payload.extend(set_word.to_le_bytes());
        payload.extend([0xDE, 0xAD]);

        let header = moved_reference(payload, flags::MOVE | flags::FORM)
            .reference_header(78)
            .unwrap()
            .unwrap();
        assert_eq!(header.form_flags, Some((mask, set_word)));
        assert_eq!(header.bytes_consumed, 33);
        assert_eq!(header.remaining_bytes, 2);
    }
}

#[test]
fn sav_54_absent_form_flag_leaves_six_following_bytes_unconsumed() {
    let mut payload = placement_prefix();
    payload.extend([1, 2, 3, 4, 5, 6]);

    let header = moved_reference(payload, flags::MOVE)
        .reference_header(78)
        .unwrap()
        .unwrap();
    assert_eq!(header.form_flags, None);
    assert_eq!(header.bytes_consumed, 27);
    assert_eq!(header.remaining_bytes, 6);
}

#[test]
fn sav_54_truncated_mask_word_is_rejected_at_every_byte_boundary() {
    for present in 0..6 {
        let mut payload = placement_prefix();
        payload.extend((0..present).map(|byte| byte as u8));

        assert!(
            moved_reference(payload, flags::MOVE | flags::FORM)
                .reference_header(78)
                .is_err()
        );
    }
}
