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

#[test]
fn sav_49_every_unnamed_flag_bit_is_retained_independently() {
    for kind in [34, 35] {
        for bit in 3..32 {
            let flags = (1_u32 << bit) | 0b111;
            let payload = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
            let form = ChangeForm {
                form: RefId::from_bytes([0x40, 1, 2]),
                change_flags: flags,
                type_field: kind,
                version: 78,
                uncompressed_len: 0,
                data: payload.to_vec(),
            };
            let typed = if kind == 34 {
                form.word_of_power_change().unwrap().unwrap()
            } else {
                form.magic_effect_change().unwrap().unwrap()
            };
            assert_eq!(typed.change_flags(), flags, "kind={kind} bit={bit}");
            assert_eq!(typed.data_bytes(), payload, "kind={kind} bit={bit}");
        }
    }
}

#[test]
fn sav_49_form_flag_bytes_are_required_only_when_the_flag_is_set() {
    for kind in [34, 35] {
        for len in 1..=6 {
            let form = ChangeForm {
                form: RefId::from_bytes([0x40, 1, 2]),
                change_flags: 0b110,
                type_field: kind,
                version: 78,
                uncompressed_len: 0,
                data: vec![0xA5; len],
            };
            let result = if kind == 34 {
                form.word_of_power_change()
            } else {
                form.magic_effect_change()
            };
            assert!(result.is_err(), "kind={kind} len={len}");
        }

        let no_form_flags = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: 0b110,
            type_field: kind,
            version: 78,
            uncompressed_len: 0,
            data: Vec::new(),
        };
        let typed = if kind == 34 {
            no_form_flags.word_of_power_change().unwrap().unwrap()
        } else {
            no_form_flags.magic_effect_change().unwrap().unwrap()
        };
        assert_eq!(typed.data_bytes(), Vec::<u8>::new());
    }
}

#[test]
fn sav_49_wrong_record_types_skip_even_malformed_compressed_data() {
    for kind in [33, 36] {
        let form = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: u32::MAX,
            type_field: kind,
            version: 78,
            uncompressed_len: 6,
            data: vec![0xFF, 0x00],
        };
        assert!(form.word_of_power_change().unwrap().is_none());
        assert!(form.magic_effect_change().unwrap().is_none());
    }
}
