// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HAZD field layout, malformed inputs and owner-relative links.

use std::{collections::HashMap, path::PathBuf};

use soe_esm::records::hazd::Hazard;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

fn hazard_data(values: [u32; 10]) -> Vec<u8> {
    values.into_iter().flat_map(u32::to_le_bytes).collect()
}

fn fields_record() -> Record {
    Record::new(b"HAZD", 0x100)
        .zsub(b"EDID", "TestHazard")
        .sub(
            b"DATA",
            hazard_data([
                7,
                2.5f32.to_bits(),
                3.5f32.to_bits(),
                4.5f32.to_bits(),
                5.5f32.to_bits(),
                0x8000_001f,
                0x0100_0800,
                0x0200_0801,
                0x0300_0802,
                0x0400_0803,
            ]),
        )
        .sub(b"MNAM", 0x0500_0804u32.to_le_bytes())
        .sub(b"ZZZZ", [0xde, 0xad])
}

#[test]
fn fmt_28_hazard_data_retains_fields_flags_and_raw_links() {
    let record = fields_record();
    let view = Hazard::from_record(&record).unwrap();
    assert_eq!(view.limit, 7);
    assert_eq!(view.radius, 2.5);
    assert_eq!(view.lifetime, 3.5);
    assert_eq!(view.image_space_radius, 4.5);
    assert_eq!(view.target_interval, 5.5);
    assert_eq!(view.flags, 0x8000_001f);
    assert_eq!(view.spell, 0x0100_0800);
    assert_eq!(view.light, 0x0200_0801);
    assert_eq!(view.impact_data_set, 0x0300_0802);
    assert_eq!(view.sound, 0x0400_0803);
    assert_eq!(view.image_space_modifier, Some(0x0500_0804));
    assert_eq!(view.editor_id.as_deref(), Some("TestHazard"));
    assert_eq!(
        record.find(Tag::new(b"ZZZZ")),
        Some([0xde, 0xad].as_slice())
    );
}

#[test]
fn fmt_28_hazard_requires_one_data_subrecord() {
    let missing = Record::new(b"HAZD", 1);
    assert!(Hazard::from_record(&missing).is_err());

    let data = hazard_data([0; 10]);
    let duplicate = Record::new(b"HAZD", 1)
        .sub(b"DATA", data.clone())
        .sub(b"DATA", data);
    assert!(Hazard::from_record(&duplicate).is_err());
}

#[test]
fn fmt_28_hazard_rejects_malformed_fixed_field_sizes() {
    let short_data = Record::new(b"HAZD", 1).sub(b"DATA", [0; 39]);
    let long_data = Record::new(b"HAZD", 1).sub(b"DATA", [0; 41]);
    let short_modifier = Record::new(b"HAZD", 1)
        .sub(b"DATA", [0; 40])
        .sub(b"MNAM", [0; 3]);
    for record in [short_data, long_data, short_modifier] {
        assert!(Hazard::from_record(&record).is_err());
    }
}

#[test]
fn fmt_28_hazard_remaps_only_form_links_inside_data() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(fields_record());
    let mapping = HashMap::from([
        (0x0100_0800, 0x0100_0900),
        (0x0200_0801, 0x0200_0901),
        (0x0300_0802, 0x0300_0902),
        (0x0400_0803, 0x0400_0903),
        (0x0500_0804, 0x0500_0904),
    ]);
    plugin.remap_form_ids(&mapping).unwrap();
    let view = Hazard::from_record(&plugin.records[0]).unwrap();
    assert_eq!(view.limit, 7);
    assert_eq!(view.radius, 2.5);
    assert_eq!(view.flags, 0x8000_001f);
    assert_eq!(view.spell, 0x0100_0900);
    assert_eq!(view.light, 0x0200_0901);
    assert_eq!(view.impact_data_set, 0x0300_0902);
    assert_eq!(view.sound, 0x0400_0903);
    assert_eq!(view.image_space_modifier, Some(0x0500_0904));
}

#[test]
fn fmt_28_hazard_links_resolve_from_the_winning_plugins_master_list() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(b"SPEL", 0x800));
    let mut other = Plugin::new(FileHeader::default());
    other.push_top(Record::new(b"LIGH", 0x801));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Other.esm".into(), "Base.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(b"HAZD", 0x0200_0100).sub(
        b"DATA",
        hazard_data([0, 0, 0, 0, 0, 0, 0x0100_0800, 0, 0, 0]),
    ));
    let mut order = LoadOrder::new();
    for (name, plugin) in [
        ("Base.esm", base),
        ("Other.esm", other),
        ("Patch.esp", patch),
    ] {
        order
            .add(name, Plugin::parse(&plugin.to_bytes()).unwrap())
            .unwrap();
    }
    let owner = order
        .winner_ref(GlobalFormId {
            file: 2,
            object: 0x100,
        })
        .unwrap();
    let record = order.record(owner);
    let view = Hazard::from_record(record).unwrap();
    let spell = order.resolve(owner.file, view.spell);
    assert_eq!(
        spell,
        GlobalFormId {
            file: 0,
            object: 0x800
        }
    );
    assert_eq!(order.winner(spell).unwrap().tag, Tag::new(b"SPEL"));
}

#[test]
fn fmt_28_five_master_hazard_winners_decode_and_links_resolve() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping HAZD survey");
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
    let mut count = 0usize;
    let mut links = 0usize;
    let mut failures = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"HAZD")) {
        if record.is_deleted() {
            continue;
        }
        count += 1;
        let owner = order.winner_ref(id).unwrap();
        match Hazard::from_record(record) {
            Ok(view) => {
                let links_to_check = [
                    (view.image_space_modifier, Some(Tag::new(b"IMAD"))),
                    (Some(view.spell), None),
                    (Some(view.light), Some(Tag::new(b"LIGH"))),
                    (Some(view.impact_data_set), Some(Tag::new(b"IPDS"))),
                    (Some(view.sound), Some(Tag::new(b"SNDR"))),
                ]
                .into_iter()
                .filter_map(|(raw, expected)| raw.map(|id| (id, expected)));
                for (raw, expected_tag) in links_to_check {
                    if raw == 0 {
                        continue;
                    }
                    links += 1;
                    let resolved = order.resolve(owner.file, raw);
                    match order.winner(resolved) {
                        Some(target)
                            if expected_tag.map_or_else(
                                || {
                                    target.tag == Tag::new(b"SPEL")
                                        || target.tag == Tag::new(b"ENCH")
                                },
                                |tag| tag == target.tag,
                            ) => {}
                        other => failures.push(format!(
                            "HAZD {id}: link {raw:08x} resolved to {:?}, expected {expected_tag:?}",
                            other.map(|target| target.tag)
                        )),
                    }
                }
            }
            Err(error) => failures.push(format!("HAZD {id}: {error}")),
        }
    }
    assert!(count > 0, "no HAZD winners surveyed");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("{count} HAZD winners decoded, {links} non-null owner-relative links resolved");
}
