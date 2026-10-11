// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::vanilla_save::references::{fixture, set_form_mask};
use soe_save::Save;

#[test]
fn sav_54_public_save_round_trip_keeps_both_words_and_the_opaque_suffix() {
    let mut save = fixture();
    let form_version = save.form_version;
    let change = &mut save.change_forms[1];
    set_form_mask(change, 0x8421_8001, 0xA55A, form_version);
    let suffix: Vec<u8> = (0..=255).cycle().take(257).collect();
    change.data.extend_from_slice(&suffix);

    let encoded = save.write().unwrap();
    let parsed = Save::parse(&encoded).unwrap();
    let change = &parsed.change_forms[1];
    let header = change.reference_header(form_version).unwrap().unwrap();

    assert_eq!(header.form_flags, Some((0x8421_8001, 0xA55A)));
    assert_eq!(header.remaining_bytes, suffix.len());
    assert_eq!(
        &change.data[header.bytes_consumed..],
        suffix.as_slice()
    );
}

#[test]
fn sav_54_public_save_round_trip_keeps_payload_when_mask_flag_is_absent() {
    let mut save = fixture();
    let form_version = save.form_version;
    let change = &mut save.change_forms[1];
    let suffix: Vec<u8> = (0..=255).rev().collect();
    change.data.extend_from_slice(&suffix);

    let encoded = save.write().unwrap();
    let parsed = Save::parse(&encoded).unwrap();
    let change = &parsed.change_forms[1];
    let header = change.reference_header(form_version).unwrap().unwrap();

    assert_eq!(header.form_flags, None);
    assert_eq!(header.remaining_bytes, suffix.len());
    assert_eq!(
        &change.data[header.bytes_consumed..],
        suffix.as_slice()
    );
}
