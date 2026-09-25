// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifies CMB-21's done check: a power attack applies its damage
//! multiplier and consumes the recorded stamina once. Independent WEAP and
//! RACE bytes, a synthetic two-actor plugin, following the fixture pattern
//! in cmb_14_stagger.rs. This file does not exercise the multiplier's
//! selection edge cases or reject paths, cmb_21_power_attack_adversarial.rs
//! covers those.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A one-handed WEAP record, damage and weight configurable
/// (crates/game/src/combat/records.rs Weapon::from_record layout).
fn weapon(object: u32, damage: u16, weight: f32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, weight.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1; // animation: one-hand sword
    put(&mut dnam, 4, 1_f32.to_bits()); // speed
    put(&mut dnam, 8, 1.1_f32.to_bits()); // reach
    put(&mut dnam, 76, 6); // skill: one-handed
    put(&mut dnam, 88, u32::MAX); // resist: -1 (no resistance type)
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

const ATTACK_FLAG_POWER: u32 = 0x04;

/// One RACE `ATKD` entry, field order and size from xEdit `wbAttackData`
/// (TES5Edit/Core/wbDefinitionsTES5.pas): damage mult, attack chance, attack
/// spell, flags, attack angle, strike angle, stagger, attack type,
/// knockdown, recovery time, stamina mult.
fn race(object: u32, damage_mult: f32, stamina_mult: f32) -> Record {
    let mut atkd = vec![0_u8; 44];
    put(&mut atkd, 0, damage_mult.to_bits());
    put(&mut atkd, 12, ATTACK_FLAG_POWER);
    put(&mut atkd, 40, stamina_mult.to_bits());
    Record::new(Tag(*b"RACE"), object)
        .sub(Tag(*b"DATA"), vec![0_u8; 124])
        .sub(Tag(*b"ATKD"), atkd)
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 2.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
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

/// Two ACHR actors, one bare NPC_ base carrying RNAM to the RACE fixture, a
/// weapon record equipped on both. Neither actor is the player, so the
/// difficulty multiplier stays 1.0 and the non-player skill GMSTs govern.
fn fixture(weapon_record: Record, weapon_id: u32, race_record: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"RNAM"), 0x700_u32.to_le_bytes()));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon_record);
    plugin.push_top(race_record);
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("PowerAttack.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 1000.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.set_actor_value(actor, "stamina", 200.).unwrap();
        world.add_item(actor, id(weapon_id), 1);
        world.actor(actor).unwrap().equipped.insert(id(weapon_id));
    }
    game
}

fn hit(attacker: u32, target: u32, weapon_id: u32, is_power_attack: bool) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(weapon_id),
        is_bash: false,
        is_power_attack,
    }
}

/// The done check's core clause: a power attack applies its RACE ATKD damage
/// multiplier to the landed damage, and takes the recorded stamina cost from
/// the attacker exactly once.
///
/// Fixture: a 20-damage, 10-weight one-handed sword swung by a full-skill
/// non-player attacker. Skill scaling (fDamageSkillMax 2.0) against an
/// unarmored target gives an ordinary hit of 20 * 2.0 = 40. The RACE's
/// power attack entry (damage_mult 1.5, stamina_mult 2.0) must scale that to
/// 60, and take (40 + weight 10 * 2) * 2.0 = 120 stamina from the attacker.
#[test]
fn cmb_21_power_attack_applies_multiplier_and_consumes_stamina_once() {
    let mut game = fixture(weapon(0xA00, 20, 10.0), 0xA00, race(0x700, 1.5, 2.0));

    let result = game.melee_hit(hit(0x900, 0x901, 0xA00, true)).unwrap();

    assert!(
        (result.health_damage - 60.0).abs() < 0.0001,
        "power attack should apply the RACE damage multiplier: got {}",
        result.health_damage
    );
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    assert!(
        (stamina - 80.0).abs() < 0.0001,
        "power attack should consume the recorded stamina exactly once: got {stamina}"
    );
}

/// Negative control: an ordinary, non-power hit with the same fixture
/// applies neither the damage multiplier nor any stamina cost, showing the
/// power attack path is an actual gate rather than always-on.
#[test]
fn cmb_21_ordinary_hit_applies_no_multiplier_and_no_stamina_cost() {
    let mut game = fixture(weapon(0xA00, 20, 10.0), 0xA00, race(0x700, 1.5, 2.0));

    let result = game.melee_hit(hit(0x900, 0x901, 0xA00, false)).unwrap();

    assert!(
        (result.health_damage - 40.0).abs() < 0.0001,
        "an ordinary hit should not apply the power attack multiplier: got {}",
        result.health_damage
    );
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    assert!(
        (stamina - 200.0).abs() < 0.0001,
        "an ordinary hit should not consume stamina: got {stamina}"
    );
}

/// Two successive power attacks each consume the recorded cost exactly
/// once, rather than a single swing being charged more than once or a
/// second swing being free.
#[test]
fn cmb_21_two_power_attacks_each_consume_the_cost_once() {
    let mut game = fixture(weapon(0xA00, 20, 10.0), 0xA00, race(0x700, 1.5, 2.0));

    game.melee_hit(hit(0x900, 0x901, 0xA00, true)).unwrap();
    game.melee_hit(hit(0x900, 0x901, 0xA00, true)).unwrap();

    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    assert!(
        (stamina - (200.0 - 2.0 * 120.0)).abs() < 0.0001,
        "each power attack should deduct the cost exactly once: got {stamina}"
    );
}
