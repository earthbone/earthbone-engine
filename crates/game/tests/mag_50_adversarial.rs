// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cure Poison selects weapon poison applications without ending other buffs.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::MeleeHit,
    inventory::{Extra, Field, Stack},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ATTACKER: u32 = 0x900;
const TARGET: u32 = 0x901;
const WEAPON: u32 = 0xA00;
const POISON: u32 = 0xB00;
const POISON_EFFECT: u32 = 0xC00;
const BUFF_EFFECT: u32 = 0xC01;
const CURE_EFFECT: u32 = 0xC02;
const BUFF_SPELL: u32 = 0xD00;
const CURE_SPELL: u32 = 0xD01;

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
}

fn effect(form: u32, flags: u32, archetype: u32, actor_value: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 64, archetype);
    put(&mut data, 68, actor_value);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(T(b"MGEF"), form).sub(T(b"DATA"), data)
}

fn spell(form: u32, effect: u32, magnitude: f32, duration: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, 1);
    put(&mut data, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn poison() -> Record {
    let mut ingestible_data = vec![0; 20];
    put(&mut ingestible_data, 4, 1 << 17);
    let mut effect_data = 15_f32.to_le_bytes().to_vec();
    effect_data.extend_from_slice(&0_u32.to_le_bytes());
    effect_data.extend_from_slice(&20_u32.to_le_bytes());
    Record::new(T(b"ALCH"), POISON)
        .sub(T(b"ENIT"), ingestible_data)
        .sub(T(b"EFID"), POISON_EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), effect_data)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(T(b"GMST"), object)
        .zsub(T(b"EDID"), name)
        .sub(T(b"DATA"), value.to_le_bytes())
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [ATTACKER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    plugin.push_top(weapon());
    plugin.push_top(effect(POISON_EFFECT, (1 << 1) | (1 << 2), 0, 26));
    plugin.push_top(effect(BUFF_EFFECT, 1 << 1, 0, 25));
    plugin.push_top(effect(CURE_EFFECT, 0, 29, u32::MAX));
    plugin.push_top(poison());
    plugin.push_top(spell(BUFF_SPELL, BUFF_EFFECT, 10.0, 60));
    plugin.push_top(spell(CURE_SPELL, CURE_EFFECT, 0.0, 0));
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1.0),
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
        plugin.push_top(gmst(0x1000 + index as u32, name, value));
    }
    let mut order = LoadOrder::new();
    order.add("CurePoison.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ATTACKER, TARGET] {
        for (name, value) in [("health", 100.0), ("stamina", 100.0), ("magicka", 100.0)] {
            game.world_mut()
                .set_actor_value(id(actor), name, value)
                .unwrap();
        }
    }
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
                    vec![Extra {
                        kind: 62,
                        fields: vec![Field::Form(Some(id(POISON))), Field::Word(1)],
                    }],
                )
                .unwrap(),
            ],
        )
        .unwrap();
    game
}

fn cast(game: &mut Game, spell: u32) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(ATTACKER),
            Some(id(ATTACKER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_50_cure_poison_removes_poison_effect_and_preserves_unrelated_buff() {
    let mut game = fixture();
    game.melee_hit(MeleeHit {
        attacker: id(ATTACKER),
        target: id(TARGET),
        weapon: id(WEAPON),
        is_bash: false,
        is_power_attack: false,
    })
    .unwrap();
    assert!(
        game.world()
            .active_effects()
            .any(|effect| effect.spell == id(POISON))
    );

    cast(&mut game, BUFF_SPELL);
    let buff = game
        .world()
        .active_effects()
        .find(|effect| effect.spell == id(BUFF_SPELL))
        .unwrap()
        .id;
    assert_eq!(game.world_mut().actor_value(id(TARGET), "magicka"), 110.0);

    cast(&mut game, CURE_SPELL);

    assert!(
        !game
            .world()
            .active_effects()
            .any(|effect| effect.spell == id(POISON))
    );
    assert!(
        game.world()
            .active_effects()
            .any(|effect| effect.id == buff)
    );
    assert_eq!(game.world_mut().actor_value(id(TARGET), "magicka"), 110.0);
}

#[test]
fn mag_50_repeated_cure_poison_is_safe_and_preserves_unrelated_buff() {
    let mut game = fixture();
    cast(&mut game, BUFF_SPELL);
    cast(&mut game, CURE_SPELL);
    cast(&mut game, CURE_SPELL);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "magicka"), 110.0);
    assert_eq!(
        game.world()
            .active_effects()
            .filter(|effect| effect.spell == id(BUFF_SPELL))
            .count(),
        1
    );
}
