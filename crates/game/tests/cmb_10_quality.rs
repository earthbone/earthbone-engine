// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifier pass for CMB-10 (tempered item quality on ordinary melee damage).
//!
//! Done check: two otherwise identical weapon instances deal quality-adjusted
//! damage using the selected stack.
//!
//! `check_combat_stack` in crates/game/src/world/combat.rs already has a
//! breaker pass in `cmb_10_quality_adversarial.rs` covering ambiguity,
//! malformed extras and nonfinite or negative values. This file exercises
//! the row's core positive claim directly: the same base weapon form, with
//! two different tempered instances swapped in as the attacker's selected
//! stack, deals two different, quality-scaled damage values, not a per-form
//! constant.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::MeleeHit,
    inventory::{Extra, Field, Stack},
};

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
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

/// Skill-scale ranges collapse to exactly 1.0 regardless of the attacker's
/// actor values, so weapon_damage before quality is exactly the WEAP
/// record's base damage (10), keeping the two hits in this file traceable
/// to one formula evaluation each.
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

/// One NPC-driven attacker (0x900) and target (0x901), so `melee_hit`'s
/// difficulty term is always 1.0 and no `fDifficulty*` GMST is needed. The
/// attacker starts with no item equipped: each hit equips exactly the
/// instance it means to exercise.
fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Quality.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
    }
    game
}

fn quality(value: f32) -> Extra {
    Extra {
        kind: 37,
        fields: vec![Field::FloatBits(value.to_bits())],
    }
}

fn stack(count: i32, extras: Vec<Extra>) -> Stack {
    Stack::new(count, extras).unwrap()
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x900),
        target: id(0x901),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// The done check: two otherwise identical weapon instances (both a single
/// count of base WEAP form 0xA00) that differ only in their ExtraHealth
/// (kind 37) tempered quality deal quality-adjusted damage keyed to
/// whichever instance the attacker has selected, not a per-form constant.
/// `Inventory::restore` replaces the item's stack list wholesale, so the
/// second `restore` call below swaps instance A out for instance B under
/// the same equipped form, exactly the "selected stack" the done check
/// names: `check_combat_stack` re-reads the currently equipped stacks on
/// every hit rather than caching a value from the first equip.
#[test]
fn cmb_10_two_identical_weapon_instances_deal_quality_adjusted_damage() {
    let mut game = fixture();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .equipped
        .insert(id(0xA00));

    // Instance A: a legendary-tempered stack (quality 2.0), selected first.
    game.world_mut()
        .reference(id(0x900))
        .unwrap()
        .inventory
        .restore(id(0xA00), 1, vec![stack(1, vec![quality(2.0)])])
        .unwrap();
    let with_a = game.melee_hit(hit()).unwrap();
    // Untempered weapon_damage is 10.0 (damage 10, scale 1.0,
    // fDamageWeaponMult 1.0, AttackDamageMult default 1.0). Instance A's
    // quality of 2.0 must double it.
    close(with_a.weapon_damage, 20.0);
    close(with_a.health_damage, 20.0);

    // Reset the target's health so the second hit reads independently of
    // the first.
    game.world_mut()
        .set_actor_value(id(0x901), "health", 100.)
        .unwrap();

    // Instance B: the same base form 0xA00, the same count, a different
    // (flawless, 1.5) tempered quality. Restoring it over the attacker's
    // inventory swaps out instance A: the equipped set still names only the
    // form, so instance B is now the selected stack that form resolves to.
    game.world_mut()
        .reference(id(0x900))
        .unwrap()
        .inventory
        .restore(id(0xA00), 1, vec![stack(1, vec![quality(1.5)])])
        .unwrap();
    let with_b = game.melee_hit(hit()).unwrap();
    // Instance B's quality of 1.5 must scale the same untempered 10.0 base
    // to 15.0, not 20.0: the selected stack's own quality is read again on
    // this hit, not cached from instance A's.
    close(with_b.weapon_damage, 15.0);
    close(with_b.health_damage, 15.0);
    assert!(
        (with_a.weapon_damage - with_b.weapon_damage).abs() > 1.0,
        "two differently tempered instances of the same weapon form must \
         deal different, quality-adjusted damage: {} vs {}",
        with_a.weapon_damage,
        with_b.weapon_damage
    );
}
