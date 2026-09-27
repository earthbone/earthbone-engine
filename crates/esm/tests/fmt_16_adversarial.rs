// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::records::refr::{Primitive, Refr};
use soe_esm::{LoadOrder, Plugin, Record, Tag};

#[test]
fn fmt_16_adversarial_each_kind_and_float_bit_pattern_round_trips() {
    for kind in 0..=4u32 {
        let mut bytes = [0u8; 32];
        let bits = [
            0x8000_0000u32,
            0x7fc0_1234,
            1,
            0x3f80_0000,
            0xbf80_0000,
            0x7fa0_0001,
            0,
            0x4000_0000,
        ];
        for (i, value) in bits.into_iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[28..].copy_from_slice(&kind.to_le_bytes());
        let view = Primitive::decode(&bytes).unwrap();
        assert_eq!(view.kind, kind);
        assert_eq!(view.encode(), bytes);
    }
}

#[test]
fn fmt_16_adversarial_reference_view_rejects_missing_base_and_keeps_unknown_type() {
    let mut bytes = [0u8; 32];
    bytes[28..].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    assert!(Refr::from_record(&Record::new(b"REFR", 1).sub(b"XPRM", bytes)).is_err());
    let view = Refr::from_record(
        &Record::new(b"REFR", 1)
            .sub(b"NAME", 2u32.to_le_bytes())
            .sub(b"XPRM", bytes),
    )
    .unwrap();
    assert_eq!(view.primitive.unwrap().kind, 0xffff_ffff);
}

#[test]
fn fmt_16_adversarial_installed_five_master_primitive_survey_round_trips() {
    let data = PathBuf::from(std::env::var_os("SKYRIM_DATA").unwrap());
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut count = 0;
    for (_, record) in order.winners_of(Tag::new(b"REFR")) {
        if let Some(bytes) = record.find(Tag::new(b"XPRM")) {
            assert_eq!(Primitive::decode(bytes).unwrap().encode().as_slice(), bytes);
            assert_eq!(
                Refr::from_record(record)
                    .unwrap()
                    .primitive
                    .unwrap()
                    .encode()
                    .as_slice(),
                bytes
            );
            count += 1;
        }
    }
    assert!(count > 0);
}
