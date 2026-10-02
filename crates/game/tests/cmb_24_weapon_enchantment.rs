// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Weapon enchantment execution and per-instance charge consumption.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::MeleeHit,
    inventory::{Extra, Field, Stack},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const WEAPON: u32 = 0xA00;
const BASE_ENCHANTMENT: u32 = 0xB00;
const INSTANCE_ENCHANTMENT: u32 = 0xB01;
const EFFECT: u32 = 0xC00;
const ATTACKER: u32 = 0x900;
const TARGET: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon() -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    Record::new(T(b"WEAP"), WEAPON)
        .sub(T(b"DATA"), data)
        .sub(T(b"DNAM"), dnam)
        .sub(T(b"EITM"), BASE_ENCHANTMENT.to_le_bytes())
        .sub(T(b"EAMT"), 500_u16.to_le_bytes())
}

fn enchantment(object: u32, cost: u32, magnitude: f32, condition: bool) -> Record {
    let mut data = vec![0; 32];
    put(&mut data, 0, cost);
    put(&mut data, 8, 1); // Fire and Forget
    put(&mut data, 12, 100); // fully charged amount
    put(&mut data, 16, 1); // Touch
    put(&mut data, 20, 6); // Enchantment
    let mut effect_data = magnitude.to_le_bytes().to_vec();
    effect_data.extend_from_slice(&0_u32.to_le_bytes());
    effect_data.extend_from_slice(&0_u32.to_le_bytes());
    let mut record = Record::new(T(b"ENCH"), object)
        .sub(T(b"ENIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), effect_data);
    if condition {
        record = record.sub(T(b"CTDA"), vec![0; 24]);
    }
    record
}

fn effect() -> Record {
    const DETRIMENTAL: u32 = 1 << 2;
    const NO_DURATION: u32 = 1 << 9;
    let mut data = vec![0; 152];
    put(&mut data, 0, DETRIMENTAL | NO_DURATION);
    put(&mut data, 64, 0); // Value Modifier
    put(&mut data, 68, 26); // stamina
    Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), data)
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.0),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 1.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.0),
        ("fArmorRatingMax", 1.0),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.0),
        ("fMaxArmorRating", 80.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x1000 + index as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
}

fn fixture(condition: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [ATTACKER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    plugin.push_top(weapon());
    plugin.push_top(enchantment(BASE_ENCHANTMENT, 1, 99.0, false));
    plugin.push_top(enchantment(INSTANCE_ENCHANTMENT, 3, 7.0, condition));
    plugin.push_top(effect());
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Enchant.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ATTACKER, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.0)
            .unwrap();
    }
    game.world_mut().add_item(id(ATTACKER), id(WEAPON), 1);
    game.world_mut()
        .actor(id(ATTACKER))
        .unwrap()
        .equipped
        .insert(id(WEAPON));
    game.world_mut()
        .reference(id(ATTACKER))
        .unwrap()
        .inventory
        .restore(
            id(WEAPON),
            1,
            vec![
                Stack::new(
                    1,
                    vec![
                        Extra {
                            kind: 155,
                            fields: vec![
                                Field::Form(Some(id(INSTANCE_ENCHANTMENT))),
                                Field::Short(100),
                            ],
                        },
                        Extra {
                            kind: 40,
                            fields: vec![Field::FloatBits(10.0_f32.to_bits())],
                        },
                    ],
                )
                .unwrap(),
            ],
        )
        .unwrap();
    game
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(ATTACKER),
        target: id(TARGET),
        weapon: id(WEAPON),
        is_bash: false,
        is_power_attack: false,
    }
}

fn current_charge(game: &Game) -> f32 {
    game.world_mut()
        .reference(id(ATTACKER))
        .unwrap()
        .inventory
        .stacks(id(WEAPON))[0]
        .extra(40)
        .and_then(|extra| match extra.fields.as_slice() {
            [Field::FloatBits(bits)] => Some(f32::from_bits(*bits)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn cmb_24_instance_enchantment_applies_once_per_hit_and_spends_its_charge() {
    let mut game = fixture(false);

    game.melee_hit(hit()).unwrap();

    assert_eq!(current_charge(&game), 7.0, "one ENIT cost is deducted");
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        93.0,
        "instance enchantment overrides the weapon's base EITM effect"
    );
    assert_eq!(game.world_mut().active_effects().count(), 1);

    game.melee_hit(hit()).unwrap();

    assert_eq!(
        current_charge(&game),
        4.0,
        "each contact spends charge once"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        86.0,
        "the recorded effect applies once on the next charged contact"
    );
    assert_eq!(game.world_mut().active_effects().count(), 2);
}

#[test]
fn cmb_24_unsupported_effect_keeps_damage_effect_and_charge_uncommitted() {
    let mut game = fixture(true);

    let error = game.melee_hit(hit()).unwrap_err();

    assert!(error.to_string().contains("weapon enchantment conditions"));
    assert_eq!(
        current_charge(&game),
        10.0,
        "failed preparation spends no charge"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("health")
            .current(),
        100.0,
        "failed enchantment preparation leaves the physical hit unapplied"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        100.0,
        "failed enchantment preparation leaves the effect unapplied"
    );
    assert_eq!(game.world_mut().active_effects().count(), 0);
}

#[test]
fn cmb_24_depleted_instance_charge_skips_effect_without_changing_charge() {
    let mut game = fixture(false);
    let selected = game
        .world_mut()
        .select_item(
            id(ATTACKER),
            id(WEAPON),
            soe_game::inventory::StackSource::Extra(0),
            1,
        )
        .unwrap();
    game.world_mut()
        .update_item(
            id(ATTACKER),
            &selected,
            vec![Extra {
                kind: 40,
                fields: vec![Field::FloatBits(2.0_f32.to_bits())],
            }],
        )
        .unwrap();

    game.melee_hit(hit()).unwrap();

    assert_eq!(
        current_charge(&game),
        2.0,
        "insufficient charge remains untouched"
    );
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        100.0,
        "an uncharged weapon hit does not apply its enchantment"
    );
    assert_eq!(game.world_mut().active_effects().count(), 0);
    assert_eq!(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("health")
            .current(),
        90.0,
        "the ordinary physical hit still lands"
    );
}
