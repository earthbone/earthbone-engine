// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId};

#[test]
fn sav_48_change_types_are_identified_without_coercing_unknown_payloads() {
    for (index, expected) in [(32, "PACK"), (33, "NAVM")] {
        let change = ChangeForm {
            form: RefId::from_bytes([0x40, 0x08, 0x00]),
            change_flags: u32::MAX,
            type_field: index,
            version: 78,
            uncompressed_len: 0,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        assert_eq!(change.record_type(), expected);
        assert_eq!(change.data, [0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
