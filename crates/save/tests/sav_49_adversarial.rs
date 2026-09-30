// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial SAV-49 checks for compressed payloads and type dispatch.

use std::io::Write;

use soe_save::{ChangeForm, RefId};

const WOOP: u8 = 34;
const MGEF: u8 = 35;

fn form(kind: u8, flags: u32, data: Vec<u8>, uncompressed_len: u32) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 1, 2]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len,
        data,
    }
}

fn decode(form: &ChangeForm) -> soe_save::Result<Option<soe_save::BaseObjectChange>> {
    match form.record_type() {
        "WOOP" => form.word_of_power_change(),
        "MGEF" => form.magic_effect_change(),
        _ => Ok(None),
    }
}

#[test]
fn sav_49_compressed_payloads_decode_and_repeated_reads_preserve_fields() {
    let bytes = [0x89, 0xAB, 0xCD, 0xEF, 0x34, 0x12];
    for kind in [WOOP, MGEF] {
        let flags = 0x8000_0007;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).unwrap();
        let change = form(kind, flags, encoder.finish().unwrap(), bytes.len() as u32);

        let first = decode(&change).unwrap().unwrap();
        let second = decode(&change).unwrap().unwrap();
        assert_eq!(first, second, "kind={kind}");
        assert_eq!(first.change_flags(), flags, "kind={kind}");
        assert_eq!(first.data_bytes(), bytes, "kind={kind}");
        assert!(first.base_object_value_changed);
        assert!(first.base_object_fullname_changed);
        assert_eq!(first.unnamed_flags, flags & !7);
        assert_eq!(first.form_flags.unwrap().flags, 0xEFCD_AB89);
        assert_eq!(first.form_flags.unwrap().unknown, 0x1234);
        assert_eq!(change.payload().unwrap().as_ref(), bytes);
    }
}

#[test]
fn sav_49_compressed_truncation_corruption_and_size_mismatch_reject() {
    for kind in [WOOP, MGEF] {
        let bytes = [1, 2, 3, 4, 5, 6];
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).unwrap();
        let mut compressed = encoder.finish().unwrap();
        compressed.pop();
        assert!(
            decode(&form(kind, 1, compressed, 6)).is_err(),
            "kind={kind}"
        );
        assert!(
            decode(&form(kind, 1, vec![0xFF, 0], 6)).is_err(),
            "kind={kind}"
        );

        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&bytes).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(decode(&form(kind, 1, compressed.clone(), 5)).is_err());
        assert!(decode(&form(kind, 1, compressed, 7)).is_err());
    }
}

#[test]
fn sav_49_each_named_change_bit_is_independent_and_payload_free() {
    for kind in [WOOP, MGEF] {
        for bits in 0_u32..8 {
            let form_bytes = if bits & 1 != 0 {
                vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
            } else {
                vec![]
            };
            let typed = decode(&form(kind, bits, form_bytes.clone(), 0))
                .unwrap()
                .unwrap();
            assert_eq!(typed.form_flags.is_some(), bits & 1 != 0);
            assert_eq!(typed.base_object_value_changed, bits & 2 != 0);
            assert_eq!(typed.base_object_fullname_changed, bits & 4 != 0);
            assert_eq!(typed.change_flags(), bits, "kind={kind} bits={bits:#x}");
            assert_eq!(typed.data_bytes(), form_bytes, "kind={kind} bits={bits:#x}");
        }
    }
}

#[test]
fn sav_49_neighbor_types_do_not_enter_woop_or_mgef_decoders() {
    for kind in [33, 36] {
        let wrong = form(kind, u32::MAX, vec![0xFF], 0);
        assert!(wrong.word_of_power_change().unwrap().is_none());
        assert!(wrong.magic_effect_change().unwrap().is_none());
    }
    assert_eq!(form(WOOP, 0, vec![], 0).record_type(), "WOOP");
    assert_eq!(form(MGEF, 0, vec![], 0).record_type(), "MGEF");
}
