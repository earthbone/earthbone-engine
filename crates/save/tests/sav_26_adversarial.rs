// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ChangeForm, RefId};

fn form(type_index: u8) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x08, 0x00]),
        change_flags: u32::MAX,
        type_field: type_index,
        version: 255,
        uncompressed_len: 19,
        data: vec![0x78, 0x9C, 0x00, 0xFF, 0x80, 0x7F],
    }
}

#[test]
fn sav_26_pcon_and_parw_keep_compressed_envelopes_distinct_and_byte_exact() {
    let pcon = form(42);
    let parw = form(47);
    let pcon_data = pcon.pcon_change().unwrap();
    let parw_data = parw.parw_change().unwrap();

    assert_eq!(pcon_data.change_flags, u32::MAX);
    assert_eq!(pcon_data.version, 255);
    assert_eq!(pcon_data.uncompressed_len, 19);
    assert_eq!(pcon_data.data, pcon.data);
    assert_eq!(parw_data.change_flags, u32::MAX);
    assert_eq!(parw_data.version, 255);
    assert_eq!(parw_data.uncompressed_len, 19);
    assert_eq!(parw_data.data, parw.data);
    assert!(pcon.parw_change().is_none());
    assert!(parw.pcon_change().is_none());
}

#[test]
fn sav_26_parw_does_not_claim_an_xedit_undefined_reference_prefix() {
    let parw = ChangeForm {
        form: RefId::from_bytes([0x40, 0x08, 0x00]),
        change_flags: 0,
        type_field: 47,
        version: 78,
        uncompressed_len: 0,
        data: vec![],
    };
    assert!(parw.reference_header(78).unwrap().is_none());
}
