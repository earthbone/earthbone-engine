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

fn perk_effect(perk: u32, entry: u8, function: u8, data_type: u8, data: &[u8]) -> Record {
    Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [entry, function, 3])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"PRKC"), [2])
        .sub(Tag(*b"EPFT"), [data_type])
        .sub(Tag(*b"EPFD"), data)
        .sub(Tag(*b"PRKF"), [])
}

fn conditioned_effect(
    perk: u32,
    entry: u8,
    function: u8,
    value: f32,
    tab: u8,
    subject: u32,
) -> Record {
    let mut condition = vec![0; 32];
    condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    condition[8..10].copy_from_slice(&72_u16.to_le_bytes());
    condition[12..16].copy_from_slice(&subject.to_le_bytes());
    let mut record = Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [entry, function, 3]);
    for current_tab in 0..3 {
        record = record.sub(Tag(*b"PRKC"), [current_tab]);
        if current_tab == tab {
            record = record.sub(Tag(*b"CTDA"), condition.clone());
        }
    }
    record
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), value.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn fixture(perks: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    for (actor, base) in [(0x900, 0x800_u32), (0x901, 0x801_u32)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), base.to_le_bytes()));
    }
    plugin.push_top(weapon());
    settings(&mut plugin);
    for perk in perks {
        plugin.push_top(perk);
    }
    let mut order = LoadOrder::new();
    order.add("Cmb09Breaker.esm", plugin).unwrap();
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
fn cmb_09_actor_value_operation_uses_each_entry_point_owner() {
    let attacker = id(0x14);
    let target = id(0x901);
    let factor = [6.0_f32.to_le_bytes(), 0.01_f32.to_le_bytes()].concat();
    let mut game = fixture(vec![
        perk_effect(0x810, 35, 14, 2, &factor),
        perk_effect(0x812, 36, 14, 2, &factor),
    ]);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x812));
    game.world_mut()
        .set_actor_value(attacker, "onehanded", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(target, "onehanded", 50.0)
        .unwrap();

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!((result.weapon_damage - 45.0).abs() < 0.001);
    assert!((result.health_damage - 45.0).abs() < 0.001);
}

#[test]
fn cmb_09_conditions_use_target_and_attacker_tabs_for_damage_entries() {
    let attacker = id(0x900);
    let target = id(0x901);
    let mut game = fixture(vec![
        conditioned_effect(0x810, 35, 2, 3.0, 2, 0x801),
        conditioned_effect(0x812, 36, 2, -4.0, 1, 0x800),
    ]);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x812));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!(
        (result.weapon_damage - 29.0).abs() < 0.001,
        "unexpected hit result: {result:?}"
    );
    assert!((result.health_damage - 29.0).abs() < 0.001);
}

#[test]
fn cmb_09_false_entry_condition_leaves_outgoing_damage_unchanged() {
    let attacker = id(0x14);
    let target = id(0x901);
    let mut game = fixture(vec![conditioned_effect(0x810, 35, 2, 100.0, 2, 0x900)]);
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
fn cmb_09_unowned_outgoing_and_incoming_effects_do_not_change_damage() {
    let attacker = id(0x14);
    let target = id(0x901);
    let mut game = fixture(vec![
        perk_effect(0x810, 35, 2, 1, &100.0_f32.to_le_bytes()),
        perk_effect(0x812, 36, 2, 1, &100.0_f32.to_le_bytes()),
    ]);
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    assert!((result.weapon_damage - 15.0).abs() < 0.001);
    assert!((result.health_damage - 15.0).abs() < 0.001);
}

#[test]
fn cmb_09_weapon_tabs_match_the_weapon_form_for_both_damage_entry_points() {
    let attacker = id(0x900);
    let target = id(0x901);
    let mut game = fixture(vec![
        conditioned_effect(0x810, 35, 2, 3.0, 1, 0xA00),
        conditioned_effect(0x812, 36, 2, -4.0, 2, 0xA00),
    ]);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));
    game.world_mut()
        .actor(target)
        .unwrap()
        .perks
        .insert(id(0x812));

    let result = game.melee_hit(hit(attacker, target)).unwrap();

    assert!((result.weapon_damage - 29.0).abs() < 0.001, "{result:?}");
    assert!((result.health_damage - 29.0).abs() < 0.001);
}

#[test]
fn cmb_09_non_finite_numeric_effect_rejects_without_committing_hit_damage() {
    let attacker = id(0x900);
    let target = id(0x901);
    let mut game = fixture(vec![perk_effect(
        0x810,
        35,
        2,
        1,
        &f32::NAN.to_le_bytes(),
    )]);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit(attacker, target));

    assert!(result.is_err());
    assert_eq!(game.world_mut().actor(target).unwrap().values.get("health").current(), 100.0);
}
