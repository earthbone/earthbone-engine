// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass for CMB-05 (shield block damage reduction).
//!
//! Contract under test, from docs/combat.md and crates/game/src/world/combat.rs:
//! - Block reduction only applies while `ActorState::blocking` is set and an
//!   equipped ARMO record occupies BOD2 shield slot bit 9 (`combat_block`).
//! - The four block game settings (fBlockSkillMult, fShieldBaseFactor,
//!   fShieldScalingFactor, fBlockMax) are read through `combat_setting`,
//!   which resolves a plugin GMST record ahead of the engine default. The
//!   done check for this row names this "record-backed multipliers"
//!   explicitly, so an overriding GMST record must change the outcome.
//! - `combat_block`'s formula clamps its result to `[0, fBlockMax]` and then
//!   `[0, 1]`, and floors a negative Block actor value to 0 before use.
//! - `combat_armor`'s equipped-item loop (`crates/game/src/world/combat.rs`)
//!   rejects overlapping equipped armor slots, including the shield slot,
//!   before `combat_block` ever runs, and this must hold even when the
//!   shield contributes nothing to armor rating (BOD2 kind 2).
//! - `blocking` is excluded from SOEW by design (world.rs doc comment on the
//!   field): a save/load cycle must reset it to false even when the actor
//!   was mid-block at save time, while the Block actor value and the
//!   equipped shield round-trip normally.
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, combat::Error};

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

/// A shield-slot ARMO record. `rating_hundredths` is the raw DNAM u32, so it
/// exercises the same low-16-bit-and-divide-by-100 decode as a real record,
/// including values a hostile or corrupt plugin could carry.
fn shield_raw(object: u32, rating_hundredths: u32, kind: u32) -> Record {
    let mut body = (1_u32 << 9).to_le_bytes().to_vec();
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

/// Baseline armor/damage settings CMB-01 already needs, mirrored from
/// combat_hits.rs so weapon_damage and armor_reduction are deterministic.
/// Block settings are deliberately left out here: each test adds only the
/// block GMSTs it means to exercise, as plugin records or as bare engine
/// defaults, to keep the override under test visible.
fn base_settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
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
        plugin.push_top(gmst(0x1000 + index as u32, name, value));
    }
}

/// One attacker (the player) and one target (0x901), both with 100 health
/// and enough one-handed skill that `fDamagePCSkillMax`/`fDamageSkillMax`
/// saturate the scale term at 1.0, isolating the block/armor reduction
/// terms this file is testing.
fn fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    base_settings(&mut plugin);
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit(attacker: u32, target: u32) -> soe_game::combat::MeleeHit {
    soe_game::combat::MeleeHit {
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

/// Sourced from the done check itself ("Block record-backed multipliers")
/// and from `combat_setting`, which checks a runtime override, then a
/// plugin GMST record, before the hardcoded engine default
/// (crates/game/src/world/forms.rs `game_setting`). A GMST record for
/// fBlockMax must cap the reduction at its own value, not the engine
/// default of 0.7, and a record for fShieldBaseFactor must change the
/// floor term. Neither block GMST is in `base_settings`, so any pass here
/// that used the engine default instead of the record would show up as a
/// value mismatch, not a missing-setting error.
#[test]
fn cmb_05_block_gmsts_are_record_backed_not_hardcoded() {
    let mut extra = vec![shield_raw(0xB00, 1000, 2)];
    // Every non-overridden block GMST still needs a value. The test supplies
    // the engine defaults explicitly for the two it does not override, so
    // the numbers below are traceable to one formula evaluation.
    extra.push(gmst(0x2000, "fBlockSkillMult", 2.0));
    extra.push(gmst(0x2001, "fShieldScalingFactor", 0.2));
    // The override under test: a plugin record for fBlockMax at 0.1, far
    // below the 0.7 engine default in game_settings.rs.
    extra.push(gmst(0x2002, "fBlockMax", 0.1));
    extra.push(gmst(0x2003, "fShieldBaseFactor", 0.45));
    let mut game = fixture(extra);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    // Uncapped formula value would be 0.48 (matching the writer's own
    // cmb_05 happy-path test at the same shield rating and Block skill).
    // With the fBlockMax record read, the result must be capped at 0.1.
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(result.blocked);
    close(result.block_reduction, 0.1);
    close(result.health_damage, 15. * (1.0 - 0.1));
}

/// Same GMST-record mechanism, isolating the base-factor term: with
/// fShieldBaseFactor overridden to 0.0 and a modest shield rating and
/// Block skill, the reduction should equal the scaling term alone, not
/// the 0.45 engine default.
#[test]
fn cmb_05_shield_base_factor_gmst_record_changes_the_floor_term() {
    let mut extra = vec![shield_raw(0xB00, 1000, 2)];
    extra.push(gmst(0x2000, "fBlockSkillMult", 2.0));
    extra.push(gmst(0x2001, "fShieldScalingFactor", 0.2));
    extra.push(gmst(0x2002, "fBlockMax", 0.7));
    extra.push(gmst(0x2003, "fShieldBaseFactor", 0.0));
    let mut game = fixture(extra);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    // reduction = 0 + 0.2/100 * 10 * (2.0 - (100-50)/100) = 0.002*10*1.5 = 0.03
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(result.blocked);
    close(result.block_reduction, 0.03);
}

/// Sourced from `combat_block`'s own `.max(0.0)` on the Block actor value
/// before it enters the skill term. A heavily damaged (debuffed) Block
/// value must floor at 0 for the formula, not carry a large negative
/// skill term through to the reduction.
#[test]
fn cmb_05_negative_block_skill_floors_at_zero_rather_than_going_negative() {
    let mut extra = vec![shield_raw(0xB00, 1000, 2)];
    extra.push(gmst(0x2000, "fBlockSkillMult", 2.0));
    extra.push(gmst(0x2001, "fShieldScalingFactor", 0.2));
    extra.push(gmst(0x2002, "fBlockMax", 0.7));
    extra.push(gmst(0x2003, "fShieldBaseFactor", 0.45));
    let mut game = fixture(extra);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();
    // Push current well below zero while leaving base at 50, the way a
    // poison or a scripted debuff would (see cmb_15's damage_actor_value
    // pattern in combat_hits.rs).
    game.world_mut()
        .damage_actor_value(id(0x901), "block", 1000.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    // Floored formula: skill=0, skill_term=2.0-1.0=1.0,
    // reduction = 0.45 + 0.002*10*1.0 = 0.47.
    // An unfloored formula would instead compute skill_term =
    // 2.0-(100-(-950))/100 = -8.5, giving reduction = 0.45-0.17 = 0.28.
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(result.blocked);
    close(result.block_reduction, 0.47);
}

/// Sourced from the DNAM decode in crates/game/src/combat/records.rs
/// (`(rating.u32()? & 0xffff) as f32 / 100.0`), which admits any 16-bit
/// value, and from `combat_block`'s `.min(fBlockMax)`. The maximum
/// encodable rating (0xFFFF, 655.35 after the /100 decode) at full Block
/// skill must still clamp at fBlockMax exactly, not overflow past 1.0 or
/// leave the earlier stages' `f64` math non-finite.
#[test]
fn cmb_05_maximum_encodable_shield_rating_still_clamps_at_block_max() {
    let mut extra = vec![shield_raw(0xB00, 0xFFFF, 2)];
    extra.push(gmst(0x2000, "fBlockSkillMult", 2.0));
    extra.push(gmst(0x2001, "fShieldScalingFactor", 0.2));
    extra.push(gmst(0x2002, "fBlockMax", 0.7));
    extra.push(gmst(0x2003, "fShieldBaseFactor", 0.45));
    let mut game = fixture(extra);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 100.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(result.blocked);
    close(result.block_reduction, 0.7);
    close(result.health_damage, 15. * (1.0 - 0.7));
}

/// Sourced from `combat_armor`'s equipped-item loop in
/// crates/game/src/world/combat.rs, which accumulates `occupied` and
/// rejects any equipped ARMO whose slots overlap an already-seen item,
/// before `combat_block` runs. Two independently equipped shield-slot
/// items, the shape a load-order or inventory-state conflict between two
/// mods could produce, must hit that guard and reject the whole hit,
/// rather than `combat_block` silently choosing the first BTreeSet entry
/// or double-counting the slot.
#[test]
fn cmb_05_two_shield_slot_items_equipped_at_once_reject_the_hit() {
    let extra = vec![shield_raw(0xB00, 1000, 2), shield_raw(0xB01, 2000, 2)];
    let mut game = fixture(extra);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut().add_item(id(0x901), id(0xB01), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB01));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    // The first rejected hit may initialize lazy record views, which the
    // melee_hit and docs/combat.md contracts allow and which save_state
    // currently persists (CMB-36 tracks that question). A second rejected
    // hit then has nothing left to initialize, so any saved-state change
    // from it is the hit itself leaking state.
    let warm = game.melee_hit(hit(0x14, 0x901));
    assert!(
        matches!(warm, Err(Error::Unsupported("overlapping equipped armor"))),
        "{warm:?}"
    );
    let before = game.save_state().unwrap();
    let result = game.melee_hit(hit(0x14, 0x901));
    assert!(
        matches!(
            result,
            Err(Error::Unsupported("overlapping equipped armor"))
        ),
        "{result:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
}

/// Sourced from the field doc on `ActorState::blocking` (world.rs): "not
/// part of saved state... the host actively re-asserts it after load".
/// An actor saved mid-block, with a shield equipped and a trained Block
/// skill, must come back from `load_state` with `blocking` cleared, so
/// the very next hit is unblocked even though every record-backed input
/// the formula needs is still present.
#[test]
fn cmb_05_load_state_clears_blocking_even_when_saved_while_blocking() {
    let extra = vec![shield_raw(0xB00, 1000, 2)];
    let mut game = fixture(extra.clone());
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    let blocked_before_save = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(blocked_before_save.blocked);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(extra);
    loaded.load_state(&saved).unwrap();
    assert!(!loaded.world().actor_ref(id(0x901)).unwrap().blocking);
    let result = loaded.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(!result.blocked);
    close(result.block_reduction, 0.);
    close(result.health_damage, 15.);
}
