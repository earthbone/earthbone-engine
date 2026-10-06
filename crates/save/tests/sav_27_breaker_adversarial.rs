// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based adversarial checks for SAV-27 spatial change payloads.

use soe_save::{ChangeForm, RefId};

fn form(kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_27_cell_rejects_partial_interior_seen_entry_with_placeholder_marker() {
    // xEdit defines interior seen data as repeated 34-byte entries. The
    // Exterior Short marker is a zero-width placeholder union, so it cannot
    // account for the trailing byte or turn a truncated entry into a suffix.
    let flags = (1 << 31) | (1 << 28);
    let result = form(6, flags, vec![0xaa]).cell_change();
    assert!(result.is_err(), "accepted partial interior seen-data entry");
}
