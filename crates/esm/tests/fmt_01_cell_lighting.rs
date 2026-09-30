// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! XCLL decoding and exact byte preservation for cell lighting fields.

use std::path::PathBuf;

use soe_esm::records::cell::{Cell, Lighting, LightingColor};
use soe_esm::{LoadOrder, Plugin, Tag};

fn xcll_92() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(92);
    for color in 0..3u8 {
        bytes.extend_from_slice(&[color, color.wrapping_add(1), color.wrapping_add(2), 0xa5]);
    }
    for bits in [0x7fc0_1234u32, 0x8000_0000] {
        bytes.extend_from_slice(&bits.to_le_bytes());
    }
    bytes.extend_from_slice(&(-123i32).to_le_bytes());
    bytes.extend_from_slice(&(456i32).to_le_bytes());
    for bits in [0xffc0_4321u32, 0x0000_0001, 0x7fa0_0001] {
        bytes.extend_from_slice(&bits.to_le_bytes());
    }
    for color in 3..9u8 {
        bytes.extend_from_slice(&[color, color.wrapping_add(1), color.wrapping_add(2), 0xa5]);
    }
    bytes.extend_from_slice(&[9, 10, 11, 0xa5]);
    bytes.extend_from_slice(&0x7fa0_0001u32.to_le_bytes());
    bytes.extend_from_slice(&[18, 19, 20, 0xa5]);
    for bits in [0x4000_0000u32, 0xdead_beef, 0x8000_0000] {
        bytes.extend_from_slice(&bits.to_le_bytes());
    }
    bytes.extend_from_slice(&0x8000_0401u32.to_le_bytes());
    bytes
}

#[test]
fn fmt_01_xcll_decodes_named_fields_and_round_trips_all_float_bits() {
    let bytes = xcll_92();
    let lighting = Lighting::decode(&bytes).unwrap();
    assert_eq!(lighting.ambient_color, LightingColor([0, 1, 2, 0xa5]));
    assert_eq!(lighting.directional_color, LightingColor([1, 2, 3, 0xa5]));
    assert_eq!(lighting.fog_near_color, LightingColor([2, 3, 4, 0xa5]));
    assert_eq!(lighting.fog_near.to_bits(), 0x7fc0_1234);
    assert_eq!(lighting.fog_far.to_bits(), 0x8000_0000);
    assert_eq!(lighting.directional_rotation_xy, -123);
    assert_eq!(lighting.directional_rotation_z, 456);
    assert_eq!(lighting.directional_fade.to_bits(), 0xffc0_4321);
    assert_eq!(lighting.fog_clip_distance.to_bits(), 0x0000_0001);
    assert_eq!(lighting.fog_power.to_bits(), 0x7fa0_0001);
    assert_eq!(lighting.ambient_colors[5], LightingColor([8, 9, 10, 0xa5]));
    assert_eq!(
        lighting.specular_color,
        Some(LightingColor([9, 10, 11, 0xa5]))
    );
    assert_eq!(lighting.fresnel_power.unwrap().to_bits(), 0x7fa0_0001);
    assert_eq!(
        lighting.fog_far_color,
        Some(LightingColor([18, 19, 20, 0xa5]))
    );
    assert_eq!(lighting.fog_max.unwrap().to_bits(), 0x4000_0000);
    assert_eq!(lighting.light_fade_begin.unwrap().to_bits(), 0xdead_beef);
    assert_eq!(lighting.light_fade_end.unwrap().to_bits(), 0x8000_0000);
    assert_eq!(lighting.inheritance_flags, Some(0x8000_0401));
    assert_eq!(lighting.encode(), bytes);
}

#[test]
fn fmt_01_xcll_accepts_optional_trailing_field_prefixes() {
    let full = xcll_92();
    for len in (64..=92).step_by(4) {
        let bytes = &full[..len];
        let lighting = Lighting::decode(bytes).unwrap();
        assert_eq!(lighting.encode(), bytes, "length {len}");
    }
}

#[test]
fn fmt_01_xcll_rejects_incomplete_trailing_fields() {
    for len in [0, 63, 65, 67, 69, 71, 73, 75, 91, 93] {
        assert!(Lighting::decode(&vec![0; len]).is_err(), "length {len}");
    }
}

#[test]
fn fmt_01_repeated_xcll_is_rejected() {
    let record = soe_esm::Record::new(b"CELL", 1)
        .sub(b"DATA", [1, 0])
        .sub(b"XCLL", [0; 64])
        .sub(b"XCLL", [0; 64]);
    assert!(Cell::from_record(&record, false).is_err());
}

#[test]
fn fmt_01_installed_cell_xcll_decodes_and_round_trips_all_bits() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping XCLL survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut with_lighting = 0usize;
    let mut shortened = 0usize;
    let mut skyrim_layout = 0usize;
    let mut errors = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"CELL")) {
        if record.is_deleted() {
            continue;
        }
        match Cell::from_record(record, false) {
            Ok(cell) => {
                if let Some(lighting) = cell.lighting {
                    with_lighting += 1;
                    let original = record.find(Tag::new(b"XCLL")).unwrap();
                    match original.len() {
                        64..=88 => shortened += 1,
                        92 => skyrim_layout += 1,
                        len => errors.push(format!("CELL {id} has unexpected XCLL size {len}")),
                    }
                    if lighting.encode() != original {
                        errors.push(format!("CELL {id} XCLL did not round-trip exactly"));
                    }
                }
            }
            Err(error) => errors.push(format!("CELL {id}: {error}")),
        }
    }
    assert!(
        with_lighting > 0,
        "no winning cells with XCLL were surveyed"
    );
    assert!(
        skyrim_layout > 0,
        "the installed Skyrim XCLL layout was not exercised"
    );
    assert!(
        errors.is_empty(),
        "{} XCLL survey failures:\n{}",
        errors.len(),
        errors.join("\n")
    );
    eprintln!("{with_lighting} XCLL cells surveyed ({shortened} shortened, {skyrim_layout} full)");
}
