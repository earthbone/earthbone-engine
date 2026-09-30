// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;

use soe_save::{ChangeForm, RefId};

fn change(kind: u8, change_flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len,
        data,
    }
}

#[test]
fn sav_43_compressed_form_flags_decode_to_exact_source_bytes() {
    for kind in [19u8, 27] {
        for payload in [
            vec![0, 0, 0, 0, 0, 0],
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        ] {
            let mut encoder = flate2::write::ZlibEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            encoder.write_all(&payload).unwrap();
            let packed = encoder.finish().unwrap();
            let form = change(kind, 1 | 2 | 4 | 0x8000_0000, packed, payload.len() as u32);
            let value = if kind == 19 {
                form.apparatus_change().unwrap().unwrap()
            } else {
                form.idle_marker_change().unwrap().unwrap()
            };
            assert_eq!(value.form_flags, Some((u32::from_le_bytes(payload[..4].try_into().unwrap()), u16::from_le_bytes(payload[4..].try_into().unwrap()))));
            assert!(value.value_changed);
            assert!(value.full_name_changed);
            assert_eq!(value.change_flags, form.change_flags);
            assert_eq!(value.data_bytes(), payload);
        }
    }
}

#[test]
fn sav_43_malformed_compressed_flags_are_rejected_for_both_types() {
    for kind in [19u8, 27] {
        let malformed = change(kind, 1, vec![0xff, 0, 0xff], 6);
        let result = if kind == 19 {
            malformed.apparatus_change()
        } else {
            malformed.idle_marker_change()
        };
        assert!(result.is_err(), "type {kind} accepted malformed zlib data");
    }
}

#[test]
fn sav_43_each_decoder_rejects_the_other_record_type() {
    let appa = change(19, 0, Vec::new(), 0);
    let idlm = change(27, 0, Vec::new(), 0);
    let unrelated = change(20, 0, Vec::new(), 0);

    assert!(appa.idle_marker_change().unwrap().is_none());
    assert!(idlm.apparatus_change().unwrap().is_none());
    assert!(unrelated.apparatus_change().unwrap().is_none());
    assert!(unrelated.idle_marker_change().unwrap().is_none());
}
