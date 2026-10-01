// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId};

#[test]
fn sav_49_change_type_length_width_bits_preserve_woop_and_mgef_dispatch() {
    for kind in [34_u8, 35] {
        for width in 0..=3_u8 {
            let flags = 0x8000_0007;
            let payload = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
            let change = ChangeForm {
                form: RefId::from_bytes([0x40, 1, 2]),
                change_flags: flags,
                type_field: (width << 6) | kind,
                version: 78,
                uncompressed_len: 0,
                data: payload.to_vec(),
            };

            assert_eq!(change.record_type(), if kind == 34 { "WOOP" } else { "MGEF" });
            let typed = if kind == 34 {
                change.word_of_power_change().unwrap().unwrap()
            } else {
                change.magic_effect_change().unwrap().unwrap()
            };
            assert_eq!(typed.change_flags(), flags, "kind={kind}, width={width}");
            assert_eq!(typed.data_bytes(), payload, "kind={kind}, width={width}");
        }
    }
}
