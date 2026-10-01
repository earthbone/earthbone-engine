// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-51 records the source and installed-corpus boundary for Concussion.

use std::collections::BTreeMap;

use soe_esm::records::gameplay::MagicEffect;
use soe_esm::{LoadOrder, Plugin, Record, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn put_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(data: &mut [u8], at: usize, value: i32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_f32(data: &mut [u8], at: usize, value: f32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn concussion_fixture() -> Record {
    let mut data = vec![0; 152];
    put_u32(&mut data, 0, 0x0102_0304);
    put_f32(&mut data, 4, 1.25);
    put_u32(&mut data, 8, 0x0506_0708);
    put_i32(&mut data, 12, 0x0102_0304);
    put_i32(&mut data, 16, 0x0506_0708);
    data[20..22].copy_from_slice(&0x1234_u16.to_le_bytes());
    put_u32(&mut data, 40, 0x1122_3344);
    put_u32(&mut data, 44, 0x5566_7788);
    put_f32(&mut data, 48, 2.5);
    put_f32(&mut data, 52, 3.5);
    put_f32(&mut data, 56, 4.5);
    put_f32(&mut data, 60, 5.5);
    put_u32(&mut data, 64, 30);
    put_i32(&mut data, 68, 0x1234_5678);
    put_u32(&mut data, 80, 1);
    put_u32(&mut data, 84, 2);
    put_i32(&mut data, 88, 0x2345_6789);
    put_f32(&mut data, 104, 6.5);
    put_u32(&mut data, 128, 0x3344_5566);
    put_u32(&mut data, 136, 0x4455_6677);

    Record::new(T(b"MGEF"), 0x51)
        .zsub(T(b"EDID"), "MAG51ConcussionFixture")
        .sub(T(b"DATA"), data)
}

#[test]
fn mag_51_concussion_named_fixture_decodes_generic_mgef_data() {
    let effect = MagicEffect::from_record(&concussion_fixture(), false).unwrap();

    assert_eq!(effect.editor_id.as_deref(), Some("MAG51ConcussionFixture"));
    assert_eq!(effect.data.flags, 0x0102_0304);
    assert_eq!(effect.data.base_cost, 1.25);
    assert_eq!(effect.data.associated_item, 0x0506_0708);
    assert_eq!(effect.data.magic_skill, 0x0102_0304);
    assert_eq!(effect.data.resistance, 0x0506_0708);
    assert_eq!(effect.data.counter_effect_count, 0x1234);
    assert_eq!(effect.data.minimum_skill, 0x1122_3344);
    assert_eq!(effect.data.area, 0x5566_7788);
    assert_eq!(effect.data.casting_time, 2.5);
    assert_eq!(effect.data.taper_curve, 3.5);
    assert_eq!(effect.data.taper_duration, 4.5);
    assert_eq!(effect.data.second_actor_value_weight, 5.5);
    assert_eq!(effect.data.archetype, 30);
    assert_eq!(effect.data.primary_actor_value, 0x1234_5678);
    assert_eq!(effect.data.cast_type, 1);
    assert_eq!(effect.data.delivery, 2);
    assert_eq!(effect.data.second_actor_value, 0x2345_6789);
    assert_eq!(effect.data.skill_usage_multiplier, 6.5);
    assert_eq!(effect.data.equip_ability, 0x3344_5566);
    assert_eq!(effect.data.perk, 0x4455_6677);
}

#[test]
fn mag_51_concussion_five_master_corpus_result() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-51 installed corpus survey");
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
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }

    let mut histogram = BTreeMap::new();
    let mut concussion = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect = MagicEffect::from_record(record, localized).unwrap();
        *histogram.entry(effect.data.archetype).or_insert(0usize) += 1;
        if effect.data.archetype == 30 {
            concussion.push((id, effect.editor_id, effect.data.associated_item));
        }
    }

    let total: usize = histogram.values().sum();
    eprintln!("MAG-51: {total} winning MGEFs, archetype counts {histogram:?}");
    eprintln!("MAG-51: archetype 30 records {concussion:?}");
    assert!(
        total > 1_000,
        "survey must cover the five-master MGEF corpus"
    );
    assert!(
        concussion.is_empty(),
        "installed five-master Concussion inventory changed: {concussion:?}"
    );
}
