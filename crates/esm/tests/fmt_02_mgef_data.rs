// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MGEF DATA offsets named by FMT-02 follow the local xEdit and UESP layouts.

use std::collections::HashMap;

use soe_esm::records::gameplay::MagicEffect;
use soe_esm::{FileHeader, Plugin, Record};

fn write_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_f32(data: &mut [u8], offset: usize, value: f32) {
    write_u32(data, offset, value.to_bits());
}

#[test]
fn fmt_02_mgef_remaining_data_fields_decode_and_round_trip() {
    let mut data = vec![0xa5; 152];
    write_f32(&mut data, 28, 0.375);
    write_u32(&mut data, 72, 0x0100_0801);
    write_u32(&mut data, 76, 0x0100_0802);
    write_u32(&mut data, 108, 0x0100_0803);
    write_f32(&mut data, 112, 1.75);
    write_u32(&mut data, 128, 0x0100_0804);
    write_u32(&mut data, 136, 0x0100_0805);
    write_f32(&mut data, 144, 125.5);
    write_f32(&mut data, 148, 2.25);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"MGEF", 0x40).sub(b"DATA", data.clone()));
    let bytes = plugin.to_bytes();
    let parsed = Plugin::parse(&bytes).unwrap();
    assert_eq!(parsed.to_bytes(), bytes);

    let effect = MagicEffect::from_record(&parsed.records[0], false).unwrap();
    assert_eq!(effect.data.taper_weight, 0.375);
    assert_eq!(effect.data.projectile, 0x0100_0801);
    assert_eq!(effect.data.explosion, 0x0100_0802);
    assert_eq!(effect.data.dual_cast_scale, 1.75);
    assert_eq!(effect.data.ai_score, 125.5);
    assert_eq!(effect.data.ai_delay_time, 2.25);
    assert_eq!(
        parsed.records[0].find(soe_esm::Tag::new(b"DATA")),
        Some(data.as_slice())
    );
}

#[test]
fn fmt_02_mgef_data_form_links_remap_at_authored_offsets() {
    let link_offsets = [
        8usize, 24, 32, 36, 72, 76, 92, 96, 100, 108, 116, 120, 124, 128, 132, 136,
    ];
    let mut mapping = HashMap::new();
    let mut data = vec![0; 152];
    for (index, offset) in link_offsets.iter().copied().enumerate() {
        let original = 0x0100_0800 + index as u32;
        let remapped = 0x0100_0900 + index as u32;
        write_u32(&mut data, offset, original);
        mapping.insert(original, remapped);
    }
    write_f32(&mut data, 28, 0.625);
    write_u32(&mut data, 68, 24);
    write_f32(&mut data, 112, 2.5);
    write_u32(&mut data, 140, 3);
    write_f32(&mut data, 144, 50.0);
    write_f32(&mut data, 148, 60.0);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"MGEF", 0x40).sub(b"DATA", data));
    plugin.remap_form_ids(&mapping).unwrap();
    let bytes = plugin.to_bytes();
    let parsed = Plugin::parse(&bytes).unwrap();
    assert_eq!(parsed.to_bytes(), bytes);

    let remapped_data = parsed.records[0].find(soe_esm::Tag::new(b"DATA")).unwrap();
    for (index, offset) in link_offsets.iter().copied().enumerate() {
        let actual = u32::from_le_bytes(remapped_data[offset..offset + 4].try_into().unwrap());
        assert_eq!(actual, 0x0100_0900 + index as u32, "link at {offset}");
    }
    let effect = MagicEffect::from_record(&parsed.records[0], false).unwrap();
    assert_eq!(effect.data.taper_weight, 0.625);
    assert_eq!(effect.data.primary_actor_value, 24);
    assert_eq!(effect.data.dual_cast_scale, 2.5);
    assert_eq!(effect.data.ai_score, 50.0);
    assert_eq!(effect.data.ai_delay_time, 60.0);
}

#[test]
fn fmt_02_mgef_invalid_data_remap_is_atomic() {
    for size in [151, 153] {
        let record = Record::new(b"MGEF", 0x40).sub(b"DATA", vec![0; size]);
        assert!(MagicEffect::from_record(&record, false).is_err());
    }

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"MGEF", 0x40).sub(b"DATA", [0; 151]));
    let before = plugin.clone();
    assert!(plugin.remap_form_ids(&HashMap::from([(1, 2)])).is_err());
    assert_eq!(plugin, before);
}
