// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-49 checks for source-byte preservation after decoding.

use std::io::Write;

use soe_save::{ChangeForm, RefId};

#[test]
fn sav_49_typed_reads_keep_compressed_change_form_bytes_unchanged() {
    for kind in [34, 35] {
        let payload = [0x89, 0xAB, 0xCD, 0xEF, 0x34, 0x12];
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&payload).unwrap();
        let form = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: 0x8000_0007,
            type_field: kind,
            version: 78,
            uncompressed_len: payload.len() as u32,
            data: encoder.finish().unwrap(),
        };
        let mut before = Vec::new();
        form.write(&mut before);

        for _ in 0..3 {
            let decoded = if kind == 34 {
                form.word_of_power_change().unwrap().unwrap()
            } else {
                form.magic_effect_change().unwrap().unwrap()
            };
            assert_eq!(decoded.data_bytes(), payload);
            assert_eq!(form.payload().unwrap().as_ref(), payload);
        }

        let mut after = Vec::new();
        form.write(&mut after);
        assert_eq!(after, before, "kind={kind}");
    }
}

#[test]
fn sav_49_rejected_trailing_payload_does_not_rewrite_the_change_form() {
    for kind in [34, 35] {
        let form = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: 0,
            type_field: kind,
            version: 78,
            uncompressed_len: 0,
            data: vec![0xAA],
        };
        let mut before = Vec::new();
        form.write(&mut before);
        let result = if kind == 34 {
            form.word_of_power_change()
        } else {
            form.magic_effect_change()
        };
        assert!(result.is_err(), "kind={kind}");

        let mut after = Vec::new();
        form.write(&mut after);
        assert_eq!(after, before, "kind={kind}");
    }
}
