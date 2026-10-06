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

fn perk(perk: u32, effects: &[(u8, u8, f32)]) -> Record {
    let mut record = Record::new(Tag(*b"PERK"), perk).sub(Tag(*b"DATA"), [0, 0, 1, 1, 0]);
    for &(rank, function, value) in effects {
        record = record
            .sub(Tag(*b"PRKE"), [2, rank, 0])
            .sub(Tag(*b"DATA"), [35, function, 3])
            .sub(Tag(*b"PRKC"), [0])
            .sub(Tag(*b"PRKC"), [1])
            .sub(Tag(*b"PRKC"), [2])
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), value.to_le_bytes())
            .sub(Tag(*b"PRKF"), []);
    }
    record
}

fn fixture(perks: Vec<Record>, npc_perk: Option<u32>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut attacker_base = Record::new(Tag(*b"NPC_"), 0x800);
    if let Some(perk) = npc_perk {
        attacker_base = attacker_base.sub(
            Tag(*b"PRKR"),
            [
                perk as u8,
                (perk >> 8) as u8,
                (perk >> 16) as u8,
                (perk >> 24) as u8,
                0,
                0,
                0,
                0,
            ],
        );
    }
    plugin.push_top(attacker_base);
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
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
    for record in perks {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Cmb09Breaker2.esm", plugin).unwrap();
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
fn cmb_09_npc_prkr_owns_attack_damage_entry() {
    let attacker = id(0x900);
    let target = id(0x901);
    let mut game = fixture(vec![perk(0x810, &[(0, 2, 5.0)])], Some(0x810));

    let result = game.melee_hit(hit(attacker, target)).unwrap();
    let baseline = fixture(Vec::new(), None)
        .melee_hit(hit(attacker, target))
        .unwrap();

    assert!(
        (result.weapon_damage - (baseline.weapon_damage + 5.0)).abs() < 0.001,
        "expected NPC-owned PRKR modifier, got {result:?} after {baseline:?}"
    );
    assert!((result.health_damage - (baseline.health_damage + 5.0)).abs() < 0.001);
}

#[test]
fn cmb_09_only_current_perk_rank_sections_apply() {
    let attacker = game_player();
    let target = id(0x901);
    let mut game = fixture(vec![perk(0x810, &[(0, 2, 2.0), (1, 2, 100.0)])], None);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));

    let result = game.melee_hit(hit(attacker, target)).unwrap();

    assert!((result.weapon_damage - 17.0).abs() < 0.001);
    assert!((result.health_damage - 17.0).abs() < 0.001);
}

fn game_player() -> GlobalFormId {
    id(0x14)
}

#[test]
fn cmb_09_repeated_hits_reapply_the_same_perk_amount_once() {
    let attacker = game_player();
    let target = id(0x901);
    let mut game = fixture(vec![perk(0x810, &[(0, 2, 3.0)])], None);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));

    let first = game.melee_hit(hit(attacker, target)).unwrap();
    let second = game.melee_hit(hit(attacker, target)).unwrap();

    assert!((first.weapon_damage - 18.0).abs() < 0.001);
    assert!((second.weapon_damage - 18.0).abs() < 0.001);
    assert!((game.world_mut().actor_value(target, "health") - 64.0).abs() < 0.001);
}

#[test]
fn cmb_09_malformed_later_effect_does_not_commit_earlier_damage() {
    let attacker = game_player();
    let target = id(0x901);
    let invalid = Record::new(Tag(*b"PERK"), 0x810)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [35, 2, 3])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"PRKC"), [2])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [35, 2, 3])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"PRKC"), [2])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), f32::NAN.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut game = fixture(vec![invalid], None);
    game.world_mut()
        .actor(attacker)
        .unwrap()
        .perks
        .insert(id(0x810));
    let health_before = game.world_mut().actor_value(target, "health");

    assert!(game.melee_hit(hit(attacker, target)).is_err());
    assert_eq!(
        game.world_mut().actor_value(target, "health"),
        health_before
    );
}
