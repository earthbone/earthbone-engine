// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass for CMB-10 (tempered item quality on ordinary melee damage).
//!
//! Contract under test, from docs/combat.md's "Tempered item quality, CMB-10"
//! section, the CK wiki's SetItemHealthPercent/GetItemHealthPercent pages and
//! `check_combat_stack` in crates/game/src/world/combat.rs:
//! - Quality is CommonLib `ExtraHealth` (extra kind 37), a single float, and
//!   applies "as a multiplier on weapon_damage" right after AttackDamageMult
//!   and before armor and shield block reduction. It has no other documented
//!   effect, and `combat_armor`'s own call to `check_combat_stack` discards
//!   the value it reads.
//! - No stack of the equipped item carrying kind 37 leaves damage unchanged
//!   (`None`, multiplier 1.0). Every stack that carries it agreeing on one
//!   finite, non-negative value returns that value. Stacks that disagree
//!   reject with `Error::Unsupported("ambiguous item quality across equipped
//!   stacks")` "rather than pick one arbitrarily", matching CMB-01's
//!   "rejected hits do not damage or emit events" atomicity contract.
//! - Quality bits go through the same `finite()` helper CMB-01 uses
//!   everywhere else, rejecting a nonfinite or negative value with
//!   `Error::Number`.
//! - The extra's fields must be exactly one `FloatBits` value, or the stack
//!   is "malformed item quality extra".
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Error, MeleeHit},
    inventory::{Extra, Field, Stack},
    Game,
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

/// A BOD2-only-relevant ARMO record. `kind` 2 (shield-type) contributes
/// nothing to armor rating or category count, isolating the quality path
/// under test from the armor-rating skill-scale formula.
fn armor(object: u32, rating_hundredths: u32, slots: u32, kind: u32) -> Record {
    let mut body = slots.to_le_bytes().to_vec();
    body.extend_from_slice(&kind.to_le_bytes());
    Record::new(Tag(*b"ARMO"), object)
        .sub(
            Tag(*b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), rating_hundredths.to_le_bytes())
        .sub(Tag(*b"BOD2"), body)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

/// Skill-scale ranges collapse to exactly 1.0 regardless of the attacker's
/// actor values, and `fArmorBaseFactor` is 0, so `combat_armor`'s category
/// term drops out. Only `fArmorScalingFactor` and the target's `damageresist`
/// actor value move the armor reduction term, keeping every number in this
/// file traceable to one formula evaluation.
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

/// Two plain NPC-driven actors (0x900 attacking 0x901), so `melee_hit`'s
/// difficulty term is always 1.0 and no `fDifficulty*` GMST is needed. No
/// item is equipped here, each test equips exactly the stacks it means to
/// exercise through `equip_plain`/`equip_stacks`.
fn fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    for record in extra {
        plugin.push_top(record);
    }
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

fn equip_plain(game: &Game, actor: u32, item: u32, count: i32) {
    game.world_mut().add_item(id(actor), id(item), count);
    game.world_mut()
        .actor(id(actor))
        .unwrap()
        .equipped
        .insert(id(item));
}

fn equip_stacks(game: &Game, actor: u32, item: u32, total: i32, stacks: Vec<Stack>) {
    game.world_mut()
        .reference(id(actor))
        .unwrap()
        .inventory
        .restore(id(item), total, stacks)
        .unwrap();
    game.world_mut()
        .actor(id(actor))
        .unwrap()
        .equipped
        .insert(id(item));
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

fn hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Sourced from docs/combat.md's CMB-10 section ("applies it as a multiplier
/// on weapon_damage") and from `combat_armor`'s own call
/// (`self.check_combat_stack(actor, item)?;`), whose returned quality is
/// discarded (`;`, not bound). Tempered quality on armor feeds no formula in
/// this file, so a target carrying a second, differently tempered, unequipped
/// copy of a piece of armor they have equipped must not lose the ability to
/// take an otherwise ordinary hit. Kind-2 (shield-type) BOD2 contributes
/// neither rating nor category count either way, isolating the quality path
/// from the armor-rating math entirely.
#[test]
fn cmb_10_ambiguous_armor_quality_does_not_block_an_unrelated_weapon_hit() {
    let mut game = fixture(vec![armor(0xB00, 100, 1 << 9, 2)]);
    equip_plain(&game, 0x900, 0xA00, 1);
    equip_stacks(
        &game,
        0x901,
        0xB00,
        2,
        vec![stack(1, vec![quality(1.2)]), stack(1, vec![quality(1.6)])],
    );
    let result = game.melee_hit(hit(0x900, 0x901)).unwrap();
    close(result.armor_rating, 0.0);
    close(result.armor_reduction, 0.0);
    close(result.health_damage, 10.0);
}

/// Sourced from docs/combat.md ("stacks that disagree return
/// Error::Unsupported... rather than pick one arbitrarily... reject the
/// contact") and from CMB-01's "rejected hits do not damage or emit events"
/// contract. Two differently tempered stacks of the attacker's own equipped
/// weapon are the row's central ambiguity case and must reject the whole
/// hit, leaving the target's saved state untouched.
#[test]
fn cmb_10_disagreeing_weapon_quality_stacks_reject_and_leave_state_untouched() {
    let mut game = fixture(vec![]);
    equip_stacks(
        &game,
        0x900,
        0xA00,
        2,
        vec![stack(1, vec![quality(1.2)]), stack(1, vec![quality(1.6)])],
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit(0x900, 0x901));
    assert!(
        matches!(
            result,
            Err(Error::Unsupported(
                "ambiguous item quality across equipped stacks"
            ))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// Sourced from docs/combat.md: quality "applies... as a multiplier on
/// weapon_damage... right after AttackDamageMult (CMB-01) and before armor
/// and shield block reduction (CMB-05)", and from the CK wiki's
/// SetItemHealthPercent/GetItemHealthPercent pages, which document 1.0 as
/// untempered and larger values as a direct multiplying percentage. A single
/// quality-bearing stack at 1.5 must scale `weapon_damage` itself (the value
/// armor then reduces), not just the final `health_damage`, and not be
/// dropped, added, or applied twice.
#[test]
fn cmb_10_single_tempered_stack_scales_weapon_damage_before_armor_reduction() {
    let mut game = fixture(vec![]);
    equip_stacks(&game, 0x900, 0xA00, 1, vec![stack(1, vec![quality(1.5)])]);
    game.world_mut()
        .set_actor_value(id(0x901), "damageresist", 50.)
        .unwrap();
    let result = game.melee_hit(hit(0x900, 0x901)).unwrap();
    // Untempered weapon_damage would read 10.0 (damage 10, scale 1.0,
    // fDamageWeaponMult 1.0, AttackDamageMult default 1.0). Quality must
    // already have scaled it to 15.0 by the time armor runs.
    close(result.weapon_damage, 15.0);
    close(result.armor_reduction, 0.06);
    // health_damage = 15.0 * (1 - 0.06) = 14.1: armor reduces the
    // quality-scaled value, not the untempered 10.0.
    close(result.health_damage, 14.1);
}

/// Sourced from `check_combat_stack`'s own contract ("malformed item quality
/// extra") for a kind-37 extra whose fields are not exactly the single
/// `FloatBits` value CommonLib's `ExtraHealth` (a lone `float health` member)
/// requires, and from CMB-01's atomicity contract. A malformed quality extra
/// must reject before any state mutates, not silently fall through to an
/// unscaled or wrongly scaled hit.
#[test]
fn cmb_10_malformed_quality_extra_rejects_without_partial_damage() {
    let mut game = fixture(vec![]);
    equip_stacks(
        &game,
        0x900,
        0xA00,
        1,
        vec![stack(
            1,
            vec![Extra {
                kind: 37,
                fields: vec![Field::Short(150)],
            }],
        )],
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit(0x900, 0x901));
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("malformed item quality extra"))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// Sourced from docs/combat.md ("Quality bits are read with the same
/// finite() helper CMB-01 already uses... rejecting a nonfinite or negative
/// value the same way a malformed game setting rejects") and from CMB-01's
/// atomicity contract. Neither a NaN or infinite payload nor a negative
/// health fraction is a valid `ExtraHealth` reading, and each must reject
/// the whole hit rather than let a corrupt multiplier reach the damage
/// formula or the target's saved health.
#[test]
fn cmb_10_nonfinite_and_negative_quality_reject_without_partial_damage() {
    for bits in [
        f32::NAN.to_bits(),
        f32::INFINITY.to_bits(),
        (-1.0_f32).to_bits(),
    ] {
        let mut game = fixture(vec![]);
        equip_stacks(
            &game,
            0x900,
            0xA00,
            1,
            vec![stack(
                1,
                vec![Extra {
                    kind: 37,
                    fields: vec![Field::FloatBits(bits)],
                }],
            )],
        );
        let before = game.save_state().unwrap();
        let result = game.melee_hit(hit(0x900, 0x901));
        assert!(
            matches!(result, Err(Error::Number)),
            "{bits:#x}: {result:?}"
        );
        assert_eq!(game.save_state().unwrap(), before);
    }
}

/// Sourced from the "engine save and load continuation" concern this row's
/// state (an inventory extra, not a new save section) falls under: a
/// tempered stack must keep scaling damage identically after a save/load
/// cycle, not lose its extra or silently revert to the untempered 1.0
/// multiplier.
#[test]
fn cmb_10_tempered_quality_survives_a_save_load_round_trip() {
    let game = fixture(vec![]);
    equip_stacks(&game, 0x900, 0xA00, 1, vec![stack(1, vec![quality(1.5)])]);
    let saved = game.save_state().unwrap();
    let mut reloaded = fixture(vec![]);
    reloaded.load_state(&saved).unwrap();
    let result = reloaded.melee_hit(hit(0x900, 0x901)).unwrap();
    close(result.weapon_damage, 15.0);
    close(result.health_damage, 15.0);
}
