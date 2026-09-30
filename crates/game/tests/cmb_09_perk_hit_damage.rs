// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn weapon() -> Record {
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8.0_f32.to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1.1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    Record::new(Tag(*b"WEAP"), 0xA00)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 3.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
}

fn float_effect(perk: u32, entry: u8, function: u8, value: f32, condition: bool) -> Record {
    let mut record = Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [entry, function, 3]);
    record = record.sub(Tag(*b"PRKC"), [0]);
    if condition {
        let mut ctda = vec![0; 32];
        ctda[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        ctda[8..10].copy_from_slice(&448_u16.to_le_bytes());
        ctda[12..16].copy_from_slice(&0x811_u32.to_le_bytes());
        record = record.sub(Tag(*b"CTDA"), ctda);
    }
    record = record.sub(Tag(*b"PRKC"), [1]).sub(Tag(*b"PRKC"), [2]);
    record
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), value.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn fixture(perks: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon());
    settings(&mut plugin);
    plugin.push_top(Record::new(Tag(*b"PERK"), 0x811));
    for perk in perks {
        plugin.push_top(perk);
    }
    let mut order = LoadOrder::new();
    order.add("Cmb09.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [game.player(), id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.0).unwrap();
        world.set_actor_value(actor, "onehanded", 100.0).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit(attacker: GlobalFormId, target: GlobalFormId) -> MeleeHit {
    MeleeHit {
        attacker,
        target,
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_09_applies_outgoing_then_incoming_effects_in_record_order() {
    let attacker = id(0x14);
    let target = id(0x901);
    let mut game = fixture(vec![
        float_effect(0x810, 35, 2, 3.0, true),
        float_effect(0x812, 35, 3, 2.0, false),
        float_effect(0x813, 36, 2, -2.0, true),
    ]);
    for perk in [0x810, 0x811, 0x812] {
        game.world_mut()
            .actor(attacker)
            .unwrap()
            .perks
            .insert(id(perk));
    }
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x813));
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x811));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!((result.weapon_damage - 34.0).abs() < 0.001);
    assert!((result.health_damage - 34.0).abs() < 0.001);
}

#[test]
fn cmb_09_ineligible_entry_leaves_the_hit_damage_unchanged() {
    let attacker = id(0x14);
    let target = id(0x901);
    let mut game = fixture(vec![float_effect(0x810, 35, 2, 100.0, true)]);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!((result.weapon_damage - 15.0).abs() < 0.001);
    assert!((result.health_damage - 15.0).abs() < 0.001);
}

#[test]
fn cmb_09_ineligible_incoming_entry_leaves_the_hit_damage_unchanged() {
    let attacker = id(0x14);
    let target = id(0x901);
    let mut game = fixture(vec![float_effect(0x810, 36, 2, -100.0, true)]);
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!((result.weapon_damage - 15.0).abs() < 0.001);
    assert!((result.health_damage - 15.0).abs() < 0.001);
}
