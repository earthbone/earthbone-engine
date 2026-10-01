// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{ActivatorChange, ChangeForm, RefId};

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
fn sav_31_adversarial_rejects_named_markers_misfiled_as_other_flags() {
    for (kind, record_type, source_bits) in [
        (10, "ACTI", 0b111),
        (11, "TACT", 0x0080_0007),
        (15, "DOOR", 0b111),
    ] {
        for bit in 0..32 {
            let mask = 1u32 << bit;
            if source_bits & mask == 0 {
                continue;
            }
            let typed = ActivatorChange {
                form_flags: None,
                value_changed: false,
                full_name_changed: false,
                speaker_changed: false,
                other_flags: mask,
            };
            assert!(
                typed.change_flags(record_type).is_err(),
                "{record_type} named marker {mask:#010x} accepted through other_flags (type {kind})"
            );
        }
    }
}

#[test]
fn sav_31_adversarial_repeated_decode_and_rebuild_stays_stable() {
    for (kind, record_type, flags, data) in [
        (10, "ACTI", 0xA5A5_0007, vec![1, 2, 3, 4, 5, 6]),
        (11, "TACT", 0xA580_0007, vec![7, 8, 9, 10, 11, 12]),
        (15, "DOOR", 0x5A5A_0005, vec![13, 14, 15, 16, 17, 18]),
    ] {
        let original = form(kind, flags, data.clone());
        for _ in 0..3 {
            let typed = original.activator_change().unwrap().unwrap();
            let rebuilt_flags = typed.change_flags(record_type).unwrap();
            let rebuilt_data = typed.data_bytes();
            assert_eq!(rebuilt_flags, flags, "{record_type}");
            assert_eq!(rebuilt_data, data, "{record_type}");
        }
    }
}

#[test]
fn sav_31_adversarial_wrong_record_types_are_not_typed_as_activators() {
    for kind in [0, 9, 12, 14, 16, 48] {
        let parsed = form(kind, u32::MAX, vec![0xFF]).activator_change();
        assert!(parsed.unwrap().is_none(), "type index {kind}");
    }
}

#[test]
fn sav_31_adversarial_rejects_invalid_compressed_payload_before_decoding() {
    for kind in [10, 11, 15] {
        let mut compressed = form(kind, 1, vec![0x78, 0x9C, 0xFF, 0x00]);
        compressed.uncompressed_len = 6;
        assert!(compressed.activator_change().is_err(), "type index {kind}");
    }
}
