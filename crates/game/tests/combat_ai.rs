// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! NPC AI data and template sources, following xEdit's NPC_ AIDT definition.

use crate::common::installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    World,
    npc::{self, AiData, Config, Stats},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn ai_data(aggression: u8) -> Vec<u8> {
    let mut data = vec![aggression, 4, 70, 1, 3, 2, 1, 0];
    for radius in [500_u32, 300, 100] {
        data.extend_from_slice(&radius.to_le_bytes());
    }
    data
}

#[test]
fn aidt_layout_and_invalid_discriminants_are_checked() {
    let expected = AiData {
        aggression: 2,
        confidence: 4,
        energy: 70,
        morality: 1,
        mood: 3,
        assistance: 2,
        aggro_radius: true,
        warn_radius: 500,
        warn_attack_radius: 300,
        attack_radius: 100,
    };
    assert_eq!(AiData::from_bytes(&ai_data(2)).unwrap(), expected);
    for field in [0, 1, 3, 4, 5, 6] {
        let mut bad = ai_data(2);
        bad[field] = 255;
        assert!(AiData::from_bytes(&bad).is_err(), "field {field}");
    }
    assert!(AiData::from_bytes(&ai_data(2)[..19]).is_err());
}

#[test]
fn ai_data_uses_its_template_group_and_modifiers_survive_reload() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"AIDT"), ai_data(2)));
    plugin.push_top(
        npc::build_record(
            0x801,
            "Child",
            "Child",
            Config {
                template_flags: npc::template::AI_DATA,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(Tag(*b"TPLT"), 0x800_u32.to_le_bytes())
        .sub(Tag(*b"AIDT"), ai_data(0)),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("CombatAI.esm", plugin).unwrap();
    let mut world = World::new(order);
    assert_eq!(world.actor_value(id(0x900), "aggression"), 2.0);
    assert_eq!(world.actor_value(id(0x900), "confidence"), 4.0);
    world
        .mod_actor_value(id(0x900), "aggression", -1.0)
        .unwrap();
    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.actor_value(id(0x900), "aggression"), 1.0);
}

#[test]
fn ai_data_follows_winning_templates_without_replacing_child_stats() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"AIDT"), ai_data(1)));
    for (object, template) in [(0x801, 0x800_u32), (0x802, 0x801)] {
        plugin.push_top(
            npc::build_record(
                object,
                "Child",
                "Child",
                Config {
                    template_flags: npc::template::AI_DATA,
                    health_offset: 75,
                    ..Config::default()
                },
                Stats::default(),
            )
            .sub(Tag(*b"TPLT"), template.to_le_bytes())
            .sub(Tag(*b"AIDT"), ai_data(0)),
        );
    }
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["CombatAI.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"AIDT"), ai_data(3)));
    let mut order = LoadOrder::new();
    order.add("CombatAI.esm", plugin).unwrap();
    order.add("CombatAIOverride.esp", patch).unwrap();
    let mut world = World::new(order);
    for (name, expected) in [
        ("aggression", 3.0),
        ("confidence", 4.0),
        ("energy", 70.0),
        ("morality", 1.0),
        ("mood", 3.0),
        ("assistance", 2.0),
    ] {
        assert_eq!(world.actor_value(id(0x900), name), expected, "{name}");
    }
    assert_eq!(world.actor_value(id(0x900), "health"), 75.0);
}

#[test]
fn installed_npc_ai_data_decodes_across_the_five_masters() {
    let Ok(_data) = std::env::var("SKYRIM_DATA") else {
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
    let mut seen = 0;
    let mut with_ai = 0;
    for (_, record) in order.winners_of(Tag(*b"NPC_")) {
        if record.is_deleted() {
            continue;
        }
        let npc = npc::Npc::from_record(record, false).unwrap();
        seen += 1;
        with_ai += usize::from(npc.ai_data.is_some());
    }
    assert!(seen >= 6000, "NPC corpus unexpectedly small: {seen}");
    assert!(with_ai >= 1000, "AIDT corpus unexpectedly small: {with_ai}");
    eprintln!("CMB-17 AIDT survey: {seen} NPCs, {with_ai} authored AIDT blocks");
}
