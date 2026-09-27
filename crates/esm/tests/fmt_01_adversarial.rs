// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::records::cell::{Cell, Lighting};
use soe_esm::{LoadOrder, Plugin, Tag};

fn xcll() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = i as u8;
    }
    bytes
}

#[test]
fn fmt_01_adversarial_all_optional_fields_follow_the_named_source_order() {
    let mut bytes = xcll();
    bytes.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // specular color
    bytes.extend_from_slice(&0x3f80_0000u32.to_le_bytes()); // Fresnel power
    bytes.extend_from_slice(&[0x55, 0x66, 0x77, 0x88]); // far fog color
    bytes.extend_from_slice(&0x4000_0000u32.to_le_bytes()); // fog max
    bytes.extend_from_slice(&0x4040_0000u32.to_le_bytes()); // fade begin
    bytes.extend_from_slice(&0x4080_0000u32.to_le_bytes()); // fade end
    bytes.extend_from_slice(&0x8000_0401u32.to_le_bytes()); // inheritance
    let view = Lighting::decode(&bytes).unwrap();
    assert_eq!(view.specular_color.unwrap().0, [0x11, 0x22, 0x33, 0x44]);
    assert_eq!(view.fresnel_power.unwrap().to_bits(), 0x3f80_0000);
    assert_eq!(view.fog_far_color.unwrap().0, [0x55, 0x66, 0x77, 0x88]);
    assert_eq!(view.fog_max.unwrap().to_bits(), 0x4000_0000);
    assert_eq!(view.light_fade_begin.unwrap().to_bits(), 0x4040_0000);
    assert_eq!(view.light_fade_end.unwrap().to_bits(), 0x4080_0000);
    assert_eq!(view.inheritance_flags, Some(0x8000_0401));
    assert_eq!(view.encode(), bytes);
}

#[test]
fn fmt_01_adversarial_every_optional_prefix_has_only_later_fields_absent() {
    let mut full = xcll();
    full.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]);
    full.extend_from_slice(&0x3f80_0000u32.to_le_bytes());
    full.extend_from_slice(&[0x55, 0x66, 0x77, 0x88]);
    full.extend_from_slice(&[0x00, 0x00, 0x00, 0x40]);
    full.extend_from_slice(&[0x00, 0x00, 0x40, 0x40]);
    full.extend_from_slice(&[0x00, 0x00, 0x80, 0x40]);
    full.extend_from_slice(&0x8000_0401u32.to_le_bytes());
    for count in 0..=7 {
        let view = Lighting::decode(&full[..64 + count * 4]).unwrap();
        assert_eq!(view.encode(), full[..64 + count * 4]);
        assert_eq!(view.specular_color.is_some(), count >= 1);
        assert_eq!(view.fresnel_power.is_some(), count >= 2);
        assert_eq!(view.fog_far_color.is_some(), count >= 3);
        assert_eq!(view.fog_max.is_some(), count >= 4);
        assert_eq!(view.light_fade_begin.is_some(), count >= 5);
        assert_eq!(view.light_fade_end.is_some(), count >= 6);
        assert_eq!(view.inheritance_flags.is_some(), count >= 7);
    }
}

#[test]
fn fmt_01_adversarial_installed_five_master_lighting_survey_round_trips() {
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
    for (_, record) in order.winners_of(Tag::new(b"CELL")) {
        if let Some(bytes) = record.find(Tag::new(b"XCLL")) {
            let cell = Cell::from_record(record, false).unwrap();
            assert_eq!(cell.lighting.unwrap().encode(), bytes);
            count += 1;
        }
    }
    assert!(count > 0);
}
