// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Footstep selection from plugin records, including sparse DLC defaults.

use crate::common::installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::footsteps::{material_name_hash, Footsteps, Gait};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SET: u32 = 0x800;
const MATERIAL: u32 = 0x900;
const IMPACT: u32 = 0xB00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|n| n.to_le_bytes()).collect()
}

fn default_entry(key: [u8; 4], value: u32) -> Vec<u8> {
    [key.as_slice(), &value.to_le_bytes()].concat()
}

fn base() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), default_entry(*b"DFTS", SET)));
    plugin.push_top(Record::new(T(b"MATT"), MATERIAL).zsub(T(b"MNAM"), "Wood"));
    plugin.push_top(
        Record::new(T(b"MATT"), MATERIAL + 1)
            .zsub(T(b"MNAM"), "Child Wood")
            .sub(T(b"PNAM"), MATERIAL.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"FSTS"), SET)
            .sub(T(b"XCNT"), words(&[1, 1, 1, 1, 1]))
            .sub(T(b"DATA"), words(&[0xA04, 0xA03, 0xA02, 0xA01, 0xA00])),
    );
    for index in 0..5 {
        plugin.push_top(
            Record::new(T(b"FSTP"), 0xA00 + index)
                .zsub(T(b"ANAM"), "FootLeft")
                .sub(T(b"DATA"), (0xC00 + index).to_le_bytes()),
        );
        plugin.push_top(
            Record::new(T(b"IPDS"), 0xC00 + index)
                .sub(T(b"PNAM"), words(&[MATERIAL, IMPACT + index])),
        );
        plugin.push_top(
            Record::new(T(b"IPCT"), IMPACT + index)
                .sub(T(b"SNAM"), (0xD00 + index).to_le_bytes())
                .sub(T(b"NAM1"), 0xE00_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(T(b"SNDR"), 0xD00 + index));
    }
    plugin.push_top(Record::new(T(b"SNDR"), 0xE00));
    plugin
}

fn order() -> LoadOrder {
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base()).unwrap();
    order
}

fn patch() -> Plugin {
    Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    })
}

#[test]
fn material_hashes_match_published_havok_ids() {
    for (name, expected) in [
        ("Wood", 500_811_281),
        ("STONE", 3_741_512_247),
        ("dirt", 3_106_094_762),
    ] {
        assert_eq!(material_name_hash(name.as_bytes()), expected);
    }
    assert_eq!(material_name_hash(b"Wood\0ignored"), 500_811_281);
    let order = order();
    assert_eq!(
        Footsteps::new(&order).material(500_811_281).unwrap(),
        Some(id(MATERIAL))
    );
    assert_eq!(Footsteps::new(&order).material(0).unwrap(), None);
}

#[test]
fn deleting_the_default_manager_does_not_resurrect_earlier_defaults() {
    let mut order = order();
    let mut addon = patch();
    addon.push_top(Record::new(T(b"DOBJ"), 0x31).with_flags(soe_esm::plugin::flags::DELETED));
    order.add("Deleted.esp", addon).unwrap();
    assert_eq!(
        Footsteps::new(&order).default_object(*b"DFTS").unwrap(),
        None
    );
}

#[test]
fn movement_groups_and_event_tags_select_distinct_impacts() {
    let order = order();
    let steps = Footsteps::new(&order);
    for (index, gait) in [Gait::Walk, Gait::Run, Gait::Sprint, Gait::Sneak, Gait::Swim]
        .into_iter()
        .enumerate()
    {
        let selected = steps
            .impacts(id(SET), gait, "FootLeft", id(MATERIAL))
            .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].impact, id(IMPACT + index as u32));
        assert_eq!(
            selected[0].sounds,
            [Some(id(0xD00 + index as u32)), Some(id(0xE00))]
        );
        assert!(steps
            .impacts(id(SET), gait, "FootRight", id(MATERIAL))
            .unwrap()
            .is_empty());
    }
}

#[test]
fn sparse_defaults_keep_earlier_keys_and_resolve_new_links_in_their_owner() {
    let mut order = order();
    let mut addon = patch();
    let mut entries = vec![0; 16];
    entries.extend(default_entry(*b"DLMT", MATERIAL));
    addon.push_top(Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), entries));
    order.add("DLC.esm", addon).unwrap();
    assert_eq!(
        Footsteps::new(&order).default_object(*b"DFTS").unwrap(),
        Some(id(SET))
    );
    assert_eq!(
        Footsteps::new(&order).default_object(*b"DLMT").unwrap(),
        Some(id(MATERIAL))
    );
    let mut addon = patch();
    addon.push_top(
        Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), default_entry(*b"DFTS", 0x0100_0900)),
    );
    order.add("Replace.esp", addon).unwrap();
    assert_eq!(
        Footsteps::new(&order).default_object(*b"DFTS").unwrap(),
        Some(GlobalFormId {
            file: 2,
            object: 0x900
        })
    );
    let mut addon = patch();
    addon.push_top(Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), default_entry(*b"DFTS", 0)));
    order.add("Silent.esp", addon).unwrap();
    assert_eq!(
        Footsteps::new(&order).default_object(*b"DFTS").unwrap(),
        None
    );
}

#[test]
fn impact_overrides_resolve_sound_links_in_the_defining_file() {
    let mut order = order();
    let mut addon = patch();
    addon.push_top(Record::new(T(b"IPCT"), IMPACT).sub(T(b"SNAM"), 0x0100_0800_u32.to_le_bytes()));
    addon.push_top(Record::new(T(b"SNDR"), 0x0100_0800));
    order.add("Sound.esp", addon).unwrap();
    let selected = Footsteps::new(&order)
        .impacts(id(SET), Gait::Walk, "FootLeft", id(MATERIAL))
        .unwrap();
    assert_eq!(
        selected[0].sounds,
        [
            Some(GlobalFormId {
                file: 1,
                object: 0x800
            }),
            None
        ]
    );
}

#[test]
fn material_parents_fall_back_and_cycles_are_reported() {
    let mut order = order();
    let steps = Footsteps::new(&order);
    assert_eq!(
        steps
            .impacts(id(SET), Gait::Walk, "FootLeft", id(MATERIAL + 1))
            .unwrap()[0]
            .impact,
        id(IMPACT)
    );
    let mut addon = patch();
    addon.push_top(
        Record::new(T(b"MATT"), MATERIAL + 1).sub(T(b"PNAM"), (MATERIAL + 1).to_le_bytes()),
    );
    order.add("Cycle.esp", addon).unwrap();
    assert!(Footsteps::new(&order)
        .impacts(id(SET), Gait::Walk, "FootLeft", id(MATERIAL + 1))
        .unwrap_err()
        .to_string()
        .contains("cycle"));
}

#[test]
fn malformed_counts_and_deleted_sound_descriptors_are_errors() {
    let mut order = order();
    let mut addon = patch();
    addon.push_top(Record::new(T(b"SNDR"), 0xD00).with_flags(soe_esm::plugin::flags::DELETED));
    order.add("Deleted.esp", addon).unwrap();
    assert!(Footsteps::new(&order)
        .impacts(id(SET), Gait::Walk, "FootLeft", id(MATERIAL))
        .is_err());
    let mut addon = patch();
    addon.push_top(
        Record::new(T(b"FSTS"), SET)
            .sub(T(b"XCNT"), words(&[u32::MAX; 5]))
            .sub(T(b"DATA"), words(&[0xA00])),
    );
    order.add("Malformed.esp", addon).unwrap();
    assert!(Footsteps::new(&order)
        .impacts(id(SET), Gait::Walk, "FootLeft", id(MATERIAL))
        .is_err());
}

#[test]
fn installed_default_footsteps_resolve_both_feet_in_each_ground_gait() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let steps = Footsteps::new(&order);
    let set = steps.default_object(*b"DFTS").unwrap().unwrap();
    assert_eq!(set, id(0x12F16));
    let mut count = 0;
    for hash in [500_811_281, 3_741_512_247, 3_106_094_762] {
        let material = steps.material(hash).unwrap().unwrap();
        for gait in [Gait::Walk, Gait::Run, Gait::Sprint, Gait::Sneak] {
            let tags = if gait == Gait::Sprint {
                ["FootSprintLeft", "FootSprintRight"]
            } else {
                ["FootLeft", "FootRight"]
            };
            for tag in tags {
                let impacts = steps.impacts(set, gait, tag, material).unwrap();
                assert!(!impacts.is_empty(), "{material} {gait:?} {tag}");
                assert!(
                    impacts
                        .iter()
                        .any(|impact| impact.sounds.iter().any(Option::is_some)),
                    "{material} {gait:?} {tag} has no sounds"
                );
                count += impacts
                    .iter()
                    .flat_map(|i| i.sounds.iter().flatten())
                    .count();
                for sound in impacts.iter().flat_map(|i| i.sounds.iter().flatten()) {
                    let playback = soe_game::sound::prepare(
                        &order,
                        *sound,
                        [0.5; 3],
                        &soe_game::sound::SoundSettings::default(),
                    )
                    .unwrap();
                    assert!(playback.data_path.starts_with("sound/fx/fst/"));
                    assert!(playback.pitch > 0.0 && playback.gain > 0.0);
                }
            }
        }
    }
    eprintln!("24 ground footstep cases resolved {count} sound descriptor links");
}
