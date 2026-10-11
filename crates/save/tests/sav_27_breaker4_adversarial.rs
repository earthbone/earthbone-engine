// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial truncation checks for source-defined SAV-27 CELL fields.

use soe_save::{ChangeForm, RefId};

fn cell(flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 0x23]),
        change_flags: flags,
        type_field: 6,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_27_cell_rejects_every_truncated_known_field_boundary() {
    for len in 0..6 {
        assert!(
            cell(1, vec![0xA5; len]).cell_change().is_err(),
            "accepted {len} of 6 common form-flag bytes"
        );
    }
    for len in 0..2 {
        assert!(
            cell(2, vec![0xA5; len]).cell_change().is_err(),
            "accepted {len} of 2 cell-flag bytes"
        );
    }
    for len in 1..32 {
        assert!(
            cell(1 << 31, vec![0xA5; len]).cell_change().is_err(),
            "accepted {len} bytes for exterior seen data"
        );
    }
    for len in 1..34 {
        if len == 32 {
            // xEdit selects the fixed exterior shape when exactly 32 bytes remain.
            continue;
        }
        assert!(
            cell(1 << 31, vec![0xA5; len]).cell_change().is_err(),
            "accepted {len} bytes for an incomplete interior entry"
        );
    }

    let mut name_prefix = vec![3, 0];
    name_prefix.extend(*b"abc");
    for len in 0..name_prefix.len() {
        assert!(
            cell(4, name_prefix[..len].to_vec()).cell_change().is_err(),
            "accepted {len} of {} name bytes",
            name_prefix.len()
        );
    }
    assert!(cell(4, vec![0, 0]).cell_change().is_ok());

    let owner = [0x40, 0x12, 0x34];
    for len in 0..owner.len() {
        assert!(
            cell(8, owner[..len].to_vec()).cell_change().is_err(),
            "accepted {len} of 3 owner RefID bytes"
        );
    }
}
