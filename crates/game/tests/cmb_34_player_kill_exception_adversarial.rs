// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-34's player-only kill exception
//! (`crates/game/src/world/combat.rs::melee_hit`, docs/combat.md "Player-only
//! kill exception for protected actors, CMB-34").
//!
//! CMB-34's own done check only names the two headline cases: a lethal
//! player hit kills a protected actor, an NPC hit still enters bleedout.
//! These tests probe the boundary the headline cases do not reach: an
//! essential actor attacked by the player, a record flagged both protected
//! and essential at once, the exact zero-health boundary, a non-lethal
//! player hit, and an NPC attacker's overkill damage.
//!
//! Fixture numbers mirror combat_bleedout_recovery.rs and its adversarial
//! file exactly: the same weapon and settings block, verified there to deal
//! 15 damage with no armor and no block for a player attacker. An NPC
//! attacker at the same skill uses the non-player skill scale and deals 30.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, npc::flags, Game};

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

fn settings(plugin: &mut Plugin) {
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
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .sub(Tag(*b"EDID"), name.as_bytes().to_vec())
                .sub(Tag(*b"DATA"), value.to_le_bytes().to_vec()),
        );
    }
}

/// One target actor (0x900) plus a baseless NPC attacker (0x901) and the
/// player, sharing the weapon fixture combat_bleedout_recovery.rs already
/// verified deals 15 damage against this settings block with no armor.
fn fixture(target_flags: u32) -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&target_flags.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Cmb34.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn player_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target: id(0x900),
        weapon: id(0xA00),
        is_power_attack: false,
    }
}

fn npc_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x901),
        target: id(0x900),
        weapon: id(0xA00),
        is_power_attack: false,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Baseline the done check names: a lethal player hit kills a merely
/// protected, non-essential target outright, and does not also leave it in
/// bleedout.
#[test]
fn cmb_34_lethal_player_hit_kills_protected_non_essential_target() {
    let mut game = fixture(flags::PROTECTED);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(player_hit()).unwrap();
    assert!(
        result.killed,
        "protected non-essential target must die to a lethal player hit"
    );
    let mut world = game.world_mut();
    let target = world.actor(id(0x900)).unwrap();
    assert!(target.dead);
    assert!(
        !target.unconscious,
        "a killed target must not also be left in bleedout"
    );
}

/// docs/combat.md states essential actors "stay gated for every attacker
/// including the player". A `player_exception` check that tested only
/// `target.protected` without also excluding `target.essential` would kill
/// this target instead of clamping it into bleedout.
#[test]
fn cmb_34_essential_target_survives_lethal_player_hit() {
    let mut game = fixture(flags::ESSENTIAL);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(player_hit()).unwrap();
    assert!(
        !result.killed,
        "essential target must not die to a player hit"
    );
    {
        let mut world = game.world_mut();
        let target = world.actor(id(0x900)).unwrap();
        assert!(!target.dead);
        assert!(
            target.unconscious,
            "essential target must still enter bleedout on a player hit"
        );
    }
    close(game.world_mut().actor_value(id(0x900), "health"), 3.);
}

/// The ck-wiki calls `SetProtected` and `SetEssential` mutually exclusive,
/// but nothing in the ACBS reader enforces that on load: `flags::PROTECTED`
/// (bit 11) and `flags::ESSENTIAL` (bit 1) are independent bits, so a
/// malformed or hand-edited plugin can still set both. `player_exception`
/// requires `!target.essential`, so essential must win here exactly as it
/// does for the essential-only case above.
#[test]
fn cmb_34_target_flagged_both_protected_and_essential_survives_player_hit() {
    let mut game = fixture(flags::PROTECTED | flags::ESSENTIAL);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(player_hit()).unwrap();
    assert!(
        !result.killed,
        "essential must win over protected for the player exception"
    );
    {
        let mut world = game.world_mut();
        let target = world.actor(id(0x900)).unwrap();
        assert!(!target.dead);
        assert!(target.unconscious);
    }
    close(game.world_mut().actor_value(id(0x900), "health"), 3.);
}

/// docs/combat.md states a non-lethal player hit on a protected target "just
/// damages them normally": the threshold clamp is skipped for the whole
/// exception path, not only once health would cross zero. A target that
/// ends up above its bleedout threshold after the hit must land exactly
/// where ordinary damage puts it, and must not enter bleedout the way an
/// NPC attacker's hit for the same damage would.
#[test]
fn cmb_34_non_lethal_player_hit_on_protected_target_is_not_clamped() {
    let mut game = fixture(flags::PROTECTED);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 20.)
        .unwrap();

    let result = game.melee_hit(player_hit()).unwrap();
    assert!(!result.killed);
    close(result.health_damage, 15.);
    {
        let mut world = game.world_mut();
        let target = world.actor(id(0x900)).unwrap();
        assert!(!target.dead);
        assert!(
            !target.unconscious,
            "a non-lethal player hit must not enter bleedout even though an NPC hit for the \
             same damage would clamp this target at its threshold"
        );
    }
    close(game.world_mut().actor_value(id(0x900), "health"), 5.);
}

/// `lethal` is `health <= 0.0`. Health landing exactly on zero must still
/// count as lethal for the player exception, not only strictly negative
/// health.
#[test]
fn cmb_34_player_hit_that_lands_exactly_on_zero_health_still_kills() {
    let mut game = fixture(flags::PROTECTED);
    // The weapon and settings fixture deals exactly 15 damage with no armor
    // (combat_bleedout_recovery.rs verified this against a debug probe), so
    // starting health at exactly 15 lands the hit on precisely zero.
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(player_hit()).unwrap();
    close(result.health_damage, 15.);
    assert!(
        result.killed,
        "health landing exactly on zero must still count as lethal"
    );
    assert!(game.world_mut().actor(id(0x900)).unwrap().dead);
}

/// An NPC attacker must never kill a protected target no matter how far the
/// raw damage would carry health past zero. An NPC-vs-NPC hit uses the
/// non-player skill scale (fDamageSkillMin/Max), which at skill 100 deals 30
/// against this fixture, twice the 15 a player hit deals with the same
/// weapon and skill. That overkill damage against a 5-health target (whose
/// bleedout threshold is 1) must still land exactly on the threshold, not
/// clip to zero and not go negative.
#[test]
fn cmb_34_npc_attacker_overkill_damage_still_clamps_at_threshold_not_zero() {
    let mut game = fixture(flags::PROTECTED);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 5.)
        .unwrap();

    let result = game.melee_hit(npc_hit()).unwrap();
    close(result.health_damage, 30.);
    assert!(
        !result.killed,
        "an NPC attacker must never kill a merely protected target"
    );
    {
        let mut world = game.world_mut();
        let target = world.actor(id(0x900)).unwrap();
        assert!(!target.dead);
        assert!(target.unconscious);
    }
    close(game.world_mut().actor_value(id(0x900), "health"), 1.);
}
