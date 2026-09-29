// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::HashMap, path::PathBuf};

use soe_esm::records::hazd::Hazard;
use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

#[test]
fn fmt_28_adversarial_hazard_links_remap_without_touching_scalar_words() {
    let values = [
        7u32,
        0x3f80_0000,
        0x4000_0000,
        0x4040_0000,
        0x4080_0000,
        0x8000_001f,
        0x0100_0800,
        0x0200_0801,
        0x0300_0802,
        0x0400_0803,
    ];
    let data = values
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(b"HAZD", 1).sub(b"DATA", data));
    plugin
        .remap_form_ids(&HashMap::from([
            (7, 70),
            (0x3f80_0000, 0xaaaaaaaa),
            (0x4000_0000, 0xbbbbbbbb),
            (0x4040_0000, 0xcccccccc),
            (0x4080_0000, 0xdddddddd),
            (0x8000_001f, 0xeeeeeeee),
            (0x0100_0800, 0x0100_0900),
            (0x0200_0801, 0x0200_0901),
            (0x0300_0802, 0x0300_0902),
            (0x0400_0803, 0x0400_0903),
        ]))
        .unwrap();
    let view = Hazard::from_record(&plugin.records[0]).unwrap();
    assert_eq!(view.limit, 7);
    assert_eq!(view.radius.to_bits(), 0x3f80_0000);
    assert_eq!(view.flags, 0x8000_001f);
    assert_eq!(view.spell, 0x0100_0900);
    assert_eq!(view.light, 0x0200_0901);
    assert_eq!(view.impact_data_set, 0x0300_0902);
    assert_eq!(view.sound, 0x0400_0903);
}

#[test]
fn fmt_28_adversarial_wrong_record_and_repeated_image_space_link_fail() {
    assert!(Hazard::from_record(&Record::new(b"SPEL", 1).sub(b"DATA", [0; 40])).is_err());
    let duplicate = Record::new(b"HAZD", 1)
        .sub(b"DATA", [0; 40])
        .sub(b"MNAM", 1u32.to_le_bytes())
        .sub(b"MNAM", 2u32.to_le_bytes());
    assert!(Hazard::from_record(&duplicate).is_err());
    let duplicate_data = Record::new(b"HAZD", 1)
        .sub(b"DATA", [0; 40])
        .sub(b"DATA", [0; 40]);
    assert!(Hazard::from_record(&duplicate_data).is_err());
}

#[test]
fn fmt_28_adversarial_installed_five_master_survey_resolves_owner_relative_links() {
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
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut hazards = 0;
    for (id, record) in order.winners_of(soe_esm::Tag::new(b"HAZD")) {
        if record.is_deleted() {
            continue;
        }
        let view = Hazard::from_record(record).unwrap();
        let owner = order.winner_ref(id).unwrap();
        for (raw, expected) in [
            (view.spell, None),
            (view.light, Some(soe_esm::Tag::new(b"LIGH"))),
            (view.impact_data_set, Some(soe_esm::Tag::new(b"IPDS"))),
            (view.sound, Some(soe_esm::Tag::new(b"SNDR"))),
            (
                view.image_space_modifier.unwrap_or(0),
                Some(soe_esm::Tag::new(b"IMAD")),
            ),
        ] {
            if raw == 0 {
                continue;
            }
            let target = order.winner(order.resolve(owner.file, raw)).unwrap();
            assert!(expected.map_or(
                target.tag == soe_esm::Tag::new(b"SPEL")
                    || target.tag == soe_esm::Tag::new(b"ENCH"),
                |tag| tag == target.tag
            ));
        }
        hazards += 1;
    }
    assert!(hazards > 0);
}
