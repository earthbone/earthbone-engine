// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

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

fn range_perk(perk: u32, entry: u8, min: f32, max: f32) -> Record {
    let mut data = min.to_le_bytes().to_vec();
    data.extend_from_slice(&max.to_le_bytes());
    Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [entry, 4, 3])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"PRKC"), [2])
        .sub(Tag(*b"EPFT"), [2])
        .sub(Tag(*b"EPFD"), data)
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
    for perk in perks {
        plugin.push_top(perk);
    }
    let mut order = LoadOrder::new();
    order.add("Cmb09Breaker3.esm", plugin).unwrap();
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

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_09_attack_damage_add_range_applies_a_fixed_authored_range() {
    let mut game = fixture(vec![range_perk(0x810, 35, 3.0, 3.0)]);
    game.world_mut()
        .actor(id(0x14))
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit()).unwrap();

    assert!((result.weapon_damage - 18.0).abs() < 0.001);
    assert!((result.health_damage - 18.0).abs() < 0.001);
}

#[test]
fn cmb_09_incoming_damage_add_range_applies_a_fixed_authored_range() {
    let mut game = fixture(vec![range_perk(0x812, 36, -2.0, -2.0)]);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .perks
        .insert(id(0x812));

    let result = game.melee_hit(hit()).unwrap();

    assert!((result.weapon_damage - 13.0).abs() < 0.001);
    assert!((result.health_damage - 13.0).abs() < 0.001);
}
