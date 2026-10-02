// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{reference::flags, ChangeForm, RefId};

fn change(mask: u32, set: u16, version: u8) -> ChangeForm {
    let mut data = vec![0x40, 0x08, 0x00];
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        data.extend(value.to_le_bytes());
    }
    data.extend(mask.to_le_bytes());
    data.extend(set.to_le_bytes());
    data.extend([0xA5, 0x5A]);
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x0A, 0x00]),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 0,
        version,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_54_mask_words_are_u32_then_u16_and_do_not_consume_following_bytes() {
    for version in [55, 77, 78] {
        for (mask, set) in [(0, 0), (u32::MAX, u16::MAX), (0x8000_0400, 0xA55A)] {
            let form = change(mask, set, version);
            let header = form.reference_header(version).unwrap().unwrap();
            assert_eq!(header.form_flags, Some((mask, set)));
            assert_eq!(header.bytes_consumed, 33);
            assert_eq!(header.remaining_bytes, 2);
            assert_eq!(header.data_bytes().unwrap(), form.data[..header.bytes_consumed]);
            assert_eq!(
                &form.data[header.bytes_consumed..],
                &[0xA5, 0x5A]
            );
        }
    }
}

#[test]
fn sav_54_mask_words_are_present_only_when_change_flag_zero_is_set() {
    let mut form = change(0x8000_0400, 0xA55A, 78);
    form.change_flags &= !flags::FORM;
    let header = form.reference_header(78).unwrap().unwrap();
    assert_eq!(header.form_flags, None);
    assert_eq!(header.bytes_consumed, 27);
    assert_eq!(header.remaining_bytes, 8);
}
