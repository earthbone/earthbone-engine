// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-15 AttackDamageMult composition and modifier validation edge cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::combat::{Error, MeleeHit};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn modifier_perk(perk: u32, entry: u8, actor_value: u32, coefficient: f32) -> Record {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&(actor_value as f32).to_le_bytes());
    data.extend_from_slice(&coefficient.to_le_bytes());
    Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [entry, 14, 3])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"PRKC"), [2])
        .sub(Tag(*b"EPFT"), [2])
        .sub(Tag(*b"EPFD"), data)
        .sub(Tag(*b"PRKF"), [])
}

fn fixture_with_perks(perks: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.0),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 1.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.0),
        ("fArmorRatingMax", 1.0),
        ("fArmorScalingFactor", 0.0),
        ("fArmorBaseFactor", 0.0),
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
    order.add("Combat.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "onehanded", 100.0)
            .unwrap();
        game.world_mut().add_item(id(actor), id(0xA00), 1);
        game.world_mut()
            .actor(id(actor))
            .unwrap()
            .equipped
            .insert(id(0xA00));
    }
    game
}

fn fixture() -> Game {
    fixture_with_perks(Vec::new())
}

fn hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

#[test]
fn cmb_15_repeated_hits_apply_outgoing_damage_multiplier_once_per_hit() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "AttackDamageMult", 1.5)
        .unwrap();
    let first = game.melee_hit(hit(0x900, 0x901)).unwrap();
    let second = game.melee_hit(hit(0x900, 0x901)).unwrap();
    assert_eq!(first.weapon_damage, 15.0);
    assert_eq!(second.weapon_damage, 15.0);
    assert_eq!(first.health_damage, 15.0);
    assert_eq!(second.health_damage, 15.0);
    assert_eq!(game.world_mut().actor_value(id(0x901), "health"), 70.0);
}

#[test]
fn cmb_15_marksman_modifier_without_perk_entry_does_not_change_damage() {
    let mut game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "MarksmanModifier", 10.0)
        .unwrap();
    let result = game.melee_hit(hit(0x900, 0x901)).unwrap();
    assert_eq!(result.weapon_damage, 10.0);
    assert_eq!(result.health_damage, 10.0);
}

#[test]
fn cmb_15_perk_actor_value_modifiers_scale_outgoing_and_incoming_damage() {
    let mut outgoing = fixture_with_perks(vec![modifier_perk(0xB10, 35, 96, 0.01)]);
    outgoing
        .world_mut()
        .set_actor_value(id(0x900), "onehandedmodifier", 20.0)
        .unwrap();
    outgoing
        .world_mut()
        .actor(id(0x900))
        .unwrap()
        .perks
        .insert(id(0xB10));
    let outgoing_result = outgoing.melee_hit(hit(0x900, 0x901)).unwrap();
    assert_eq!(outgoing_result.weapon_damage, 12.0);
    assert_eq!(outgoing_result.health_damage, 12.0);

    let mut incoming = fixture_with_perks(vec![modifier_perk(0xB11, 36, 102, -0.01)]);
    incoming
        .world_mut()
        .set_actor_value(id(0x901), "lightarmormodifier", 20.0)
        .unwrap();
    incoming
        .world_mut()
        .actor(id(0x901))
        .unwrap()
        .perks
        .insert(id(0xB11));
    let incoming_result = incoming.melee_hit(hit(0x900, 0x901)).unwrap();
    assert_eq!(incoming_result.weapon_damage, 8.0);
    assert_eq!(incoming_result.health_damage, 8.0);
}

#[test]
fn cmb_15_perk_modifier_does_not_bypass_equipment_validation() {
    let mut game = fixture_with_perks(vec![modifier_perk(0xB10, 35, 96, 0.01)]);
    game.world_mut()
        .set_actor_value(id(0x900), "onehandedmodifier", 20.0)
        .unwrap();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .perks
        .insert(id(0xB10));
    game.world_mut()
        .reference(id(0x900))
        .unwrap()
        .inventory
        .clear();
    let health = game.world_mut().actor_value(id(0x901), "health");
    assert!(matches!(
        game.melee_hit(hit(0x900, 0x901)),
        Err(Error::Equipment)
    ));
    assert_eq!(game.world_mut().actor_value(id(0x901), "health"), health);
}
