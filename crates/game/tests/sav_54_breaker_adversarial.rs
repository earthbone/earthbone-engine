// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId, reference::flags};

fn moved(mask: u32, second: u16, version: u8) -> ChangeForm {
    let mut data = vec![0x40, 0x08, 0x00];
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        data.extend(value.to_le_bytes());
    }
    data.extend(mask.to_le_bytes());
    data.extend(second.to_le_bytes());
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x0a, 0x00]),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 0,
        version,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_54_mask_words_remain_distinct_unsigned_fields_at_supported_layout_versions() {
    for version in [55, 78] {
        for (mask, second) in [
            (0x8000_0400, 0x8000),
            (0x0000_0c00, 0x7fff),
            (u32::MAX, u16::MAX),
        ] {
            let change = moved(mask, second, version);
            let header = change.reference_header(version).unwrap().unwrap();
            assert_eq!(header.form_flags, Some((mask, second)));
            assert_eq!(header.bytes_consumed, 33);
            assert_eq!(header.data_bytes().unwrap(), change.data);
        }
    }
}

#[test]
fn sav_54_flag_presence_and_payload_truncation_follow_xedit_union_layout() {
    let mut absent = moved(0, 0, 78);
    absent.change_flags = flags::MOVE;
    let absent_header = absent.reference_header(78).unwrap().unwrap();
    assert_eq!(absent_header.form_flags, None);
    assert_eq!(absent_header.bytes_consumed, 27);

    for length in 0..6 {
        let mut truncated = moved(0, 0, 78);
        truncated.data.truncate(27 + length);
        assert!(truncated.reference_header(78).is_err(), "length={length}");
    }
}
