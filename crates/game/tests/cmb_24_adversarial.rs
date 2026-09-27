// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Boundary checks for enchantment effects and instance charge persistence.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::MeleeHit,
    inventory::{Extra, Field, Stack},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const W: u32 = 0xA00;
const ENCH: u32 = 0xB01;
const EFFECT: u32 = 0xC00;
const A: u32 = 0x900;
const V: u32 = 0x901;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [A, V] {
        p.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    let mut wd = vec![0; 10];
    put(&mut wd, 4, 8_f32.to_bits());
    let mut wn = vec![0; 100];
    wn[0] = 1;
    put(&mut wn, 4, 1_f32.to_bits());
    put(&mut wn, 8, 1_f32.to_bits());
    put(&mut wn, 76, 6);
    put(&mut wn, 88, u32::MAX);
    p.push_top(
        Record::new(T(b"WEAP"), W)
            .sub(T(b"DATA"), wd)
            .sub(T(b"DNAM"), wn)
            .sub(T(b"EAMT"), 500_u16.to_le_bytes()),
    );
    let mut enit = vec![0; 32];
    put(&mut enit, 0, 3);
    put(&mut enit, 8, 1);
    put(&mut enit, 12, 100);
    put(&mut enit, 16, 1);
    put(&mut enit, 20, 6);
    let mut effect_item = 7_f32.to_le_bytes().to_vec();
    effect_item.extend_from_slice(&0_u32.to_le_bytes());
    effect_item.extend_from_slice(&0_u32.to_le_bytes());
    p.push_top(
        Record::new(T(b"ENCH"), ENCH)
            .sub(T(b"ENIT"), enit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), effect_item),
    );
    let mut ed = vec![0; 152];
    put(&mut ed, 0, (1 << 2) | (1 << 9));
    put(&mut ed, 64, 0);
    put(&mut ed, 68, 26);
    p.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), ed));
    for (i, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 1.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.),
        ("fArmorRatingMax", 1.),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        p.push_top(
            Record::new(T(b"GMST"), 0x1000 + i as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Charge.esm", p).unwrap();
    let game = Game::new(order);
    for actor in [A, V] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.)
            .unwrap();
    }
    game.world_mut().add_item(id(A), id(W), 1);
    game.world_mut()
        .actor(id(A))
        .unwrap()
        .equipped
        .insert(id(W));
    game.world_mut()
        .reference(id(A))
        .unwrap()
        .inventory
        .restore(
            id(W),
            1,
            vec![Stack::new(
                1,
                vec![
                    Extra {
                        kind: 155,
                        fields: vec![Field::Form(Some(id(ENCH))), Field::Short(100)],
                    },
                    Extra {
                        kind: 40,
                        fields: vec![Field::FloatBits(10_f32.to_bits())],
                    },
                ],
            )
            .unwrap()],
        )
        .unwrap();
    game
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(A),
        target: id(V),
        weapon: id(W),
        is_bash: false,
        is_power_attack: false,
    }
}
fn charge(game: &Game) -> f32 {
    game.world_mut()
        .reference(id(A))
        .unwrap()
        .inventory
        .stacks(id(W))[0]
        .extra(40)
        .and_then(|e| match e.fields.as_slice() {
            [Field::FloatBits(x)] => Some(f32::from_bits(*x)),
            _ => None,
        })
        .unwrap()
}

#[test]
fn instance_charge_and_effect_count_survive_engine_save_load() {
    let mut game = fixture();
    game.melee_hit(hit()).unwrap();
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert_eq!(charge(&game), 7.);
    assert_eq!(game.world_mut().active_effects().count(), 1);
    game.melee_hit(hit()).unwrap();
    assert_eq!(charge(&game), 4.);
    assert_eq!(
        game.world_mut()
            .actor(id(V))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        86.
    );
}

#[test]
fn multiple_enchanted_instances_are_rejected_without_damage_or_charge_spend() {
    let mut game = fixture();
    let extras = vec![
        Extra {
            kind: 155,
            fields: vec![Field::Form(Some(id(ENCH))), Field::Short(100)],
        },
        Extra {
            kind: 40,
            fields: vec![Field::FloatBits(10_f32.to_bits())],
        },
    ];
    game.world_mut()
        .reference(id(A))
        .unwrap()
        .inventory
        .restore(
            id(W),
            2,
            vec![
                Stack::new(1, extras.clone()).unwrap(),
                Stack::new(1, extras).unwrap(),
            ],
        )
        .unwrap();

    assert!(game.melee_hit(hit()).is_err());
    assert_eq!(
        game.world_mut()
            .actor(id(V))
            .unwrap()
            .values
            .get("health")
            .current(),
        100.
    );
    assert_eq!(game.world_mut().active_effects().count(), 0);
}
