// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifier pass for CMB-25 (weapon poison consumption on a successful hit).
//!
//! Done check: the selected poisoned stack applies its poison once and
//! clears the consumed coating while other stacks stay unchanged.
//!
//! `combat_stack_poison` and `apply_poison` in
//! crates/game/src/world/combat.rs and crates/game/src/world/magic.rs have a
//! breaker pass in cmb_25_weapon_poison_adversarial.rs covering ambiguity, a
//! malformed extra and the bash exclusion. This file exercises the row's
//! core positive claim: a hit against a weapon carrying a poisoned stack
//! (CommonLib ExtraPoison, kind 62) applies the poison's magic effect to the
//! target exactly once, clears that stack's kind-62 extra, and leaves a
//! second, unpoisoned stack of the same weapon form untouched.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::MeleeHit,
    inventory::{Extra, Field, Stack},
    Game,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const WEAPON: u32 = 0xA00;
const POISON: u32 = 0xB00;
const EFFECT: u32 = 0xC00;
const ATTACKER: u32 = 0x900;
const TARGET: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
    Record::new(T(b"WEAP"), object)
        .sub(T(b"DATA"), data)
        .sub(T(b"DNAM"), dnam)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(T(b"GMST"), object)
        .zsub(T(b"EDID"), name)
        .sub(T(b"DATA"), value.to_le_bytes())
}

/// Skill-scale ranges collapse to exactly 1.0, matching cmb_10_quality.rs, so
/// the poison assertions below can isolate the stamina drop from the
/// weapon's own damage formula.
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
        plugin.push_top(gmst(0x1000 + index as u32, name, value));
    }
}

/// A Value Modifier (archetype 0) MGEF on stamina, detrimental and flagged
/// NO_DURATION (bit 9) so `ingestible_prepared_effects` reports
/// `duration: Some(0.0)` regardless of the EFIT entry, which
/// `apply_prepared_effects` applies immediately rather than as a
/// damage-over-time tick. Stamina, not health, keeps this test clear of the
/// bleedout and death paths `apply_effect_values` carries for health.
fn poison_effect() -> Record {
    const DETRIMENTAL: u32 = 1 << 2;
    const NO_DURATION: u32 = 1 << 9;
    let mut data = vec![0; 152];
    put(&mut data, 0, DETRIMENTAL | NO_DURATION);
    put(&mut data, 64, 0); // archetype: Value Modifier
    put(&mut data, 68, 26); // primary actor value: stamina
    Record::new(T(b"MGEF"), EFFECT)
        .zsub(T(b"EDID"), "PoisonEffect")
        .sub(T(b"DATA"), data)
}

/// An ALCH marked as a weapon poison (ENIT flags bit 17, `INGESTIBLE_POISON`)
/// with a single EFID/EFIT entry against `poison_effect`. `apply_poison`
/// does not itself check this flag (see its doc comment), the record still
/// carries it for fidelity with a real poison plugin entry.
fn poison_alch(magnitude: f32) -> Record {
    const INGESTIBLE_POISON: u32 = 1 << 17;
    let mut flags_block = vec![0; 20];
    put(&mut flags_block, 4, INGESTIBLE_POISON);
    let mut magnitude_block = magnitude.to_le_bytes().to_vec();
    magnitude_block.extend_from_slice(&0u32.to_le_bytes());
    magnitude_block.extend_from_slice(&0u32.to_le_bytes());
    Record::new(T(b"ALCH"), POISON)
        .zsub(T(b"EDID"), "TestPoison")
        .sub(T(b"ENIT"), flags_block)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), magnitude_block)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [ATTACKER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    plugin.push_top(weapon(WEAPON, 10, 6));
    plugin.push_top(poison_effect());
    plugin.push_top(poison_alch(15.0));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Poison.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ATTACKER, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.)
            .unwrap();
    }
    game
}

fn poison_extra(item: GlobalFormId, count: u32) -> Extra {
    Extra {
        kind: 62,
        fields: vec![Field::Form(Some(item)), Field::Word(count)],
    }
}

fn stack(count: i32, extras: Vec<Extra>) -> Stack {
    Stack::new(count, extras).unwrap()
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

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// The done check, all three clauses in one flow: the attacker's weapon has
/// two stacks, index 0 poisoned (count 1) and index 1 plain (count 1, no
/// extras). The equipped set is form-only (CMB-03), so `combat_stack_poison`
/// must read both stacks and select the one that actually carries the
/// kind-62 extra rather than rejecting on the presence of the second, since
/// only one of the two stacks is ambiguous-poisoned, not two.
#[test]
fn cmb_25_poisoned_stack_applies_once_and_clears_while_other_stack_is_unchanged() {
    let mut game = fixture();
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
            2,
            vec![
                stack(1, vec![poison_extra(id(POISON), 1)]),
                stack(1, vec![]),
            ],
        )
        .unwrap();

    let result = game.melee_hit(hit()).unwrap();

    // Clause 1: the poison applied. `result.poisoned` reports it, and the
    // target's stamina dropped by exactly the ALCH's magnitude (15.0), the
    // one actor value this fixture's MGEF touches.
    assert!(
        result.poisoned,
        "a poisoned stack must report poisoned=true"
    );
    close(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        85.0,
    );

    // Clause 2: the consumed coating is cleared from exactly the stack that
    // carried it, count untouched.
    let stacks = game
        .world_mut()
        .reference(id(ATTACKER))
        .unwrap()
        .inventory
        .stacks(id(WEAPON))
        .to_vec();
    assert!(
        stacks[0].extra(62).is_none(),
        "the selected stack's poison extra must be cleared after the hit"
    );
    assert_eq!(
        stacks[0].count(),
        1,
        "clearing poison must not change the stack's count"
    );

    // Clause 3: the other stack of the same weapon form is unchanged.
    assert!(stacks[1].extras().is_empty());
    assert_eq!(stacks[1].count(), 1);

    // A second hit with the coating already cleared must not apply the
    // poison again: stamina holds at 85.0 (only the weapon's own health
    // damage changes on this hit, stamina is untouched by that formula).
    let second = game.melee_hit(hit()).unwrap();
    assert!(
        !second.poisoned,
        "a second hit after the coating is cleared must not reapply poison"
    );
    close(
        game.world_mut()
            .actor(id(TARGET))
            .unwrap()
            .values
            .get("stamina")
            .current(),
        85.0,
    );
}
