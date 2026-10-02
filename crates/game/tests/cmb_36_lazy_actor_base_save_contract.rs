// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Verifier pass for CMB-36 (docs/combat.md "Application and events",
//! `crates/game/src/world/save.rs` `save_state`'s `actor_bases` comment).
//!
//! The chosen contract: a rejected first melee hit that reads
//! protected/essential/invulnerable through `World::actor_base` persists
//! that lazily-built `ActorBaseState` into `save_state`, the same
//! read-through, save-whole-map pattern every other lazy `World` cache
//! (globals, leveled lists, factions, cells) already follows. There is no
//! rejected-only subset excluded from serialization.
//!
//! This test drives that contract from the public API only: it never reads
//! `World::actor_bases` directly (private, and out of scope for a
//! verifier), it compares the serialized `save_state()` bytes before and
//! after a rejected hit that must populate the cache.
//!
//! This file only adds a test. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Error, combat::MeleeHit, npc::flags, Game};

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

/// One invulnerable target actor (0x900) on base 0x800, so the first melee
/// hit against it is rejected by the invulnerable check in `melee_hit`
/// after `World::actor_base` has already been called to read
/// protected/essential. The player (attacker) and target both carry the
/// weapon so the rejection comes from the invulnerable flag, not equipment.
fn fixture() -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&flags::INVULNERABLE.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Cmb36Lazy.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    let mut world = game.world_mut();
    world.set_actor_value(player, "onehanded", 100.).unwrap();
    world.add_item(player, id(0xA00), 1);
    world.actor(player).unwrap().equipped.insert(id(0xA00));
    drop(world);
    game
}

fn player_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

/// A rejected first melee hit against an invulnerable target must populate
/// the target base's `actor_bases` entry through `World::actor_base` (read
/// for the protected/essential check ahead of the invulnerable check that
/// then rejects the hit), and that population must be visible in
/// `save_state()`: the serialized bytes must differ, and grow, from a save
/// taken before the target was ever hit. A save/load round trip on the
/// post-hit bytes must also succeed, confirming the persisted entry is
/// well-formed, not just present as raw length.
#[test]
fn cmb_36_rejected_first_hit_persists_lazy_actor_base() {
    let mut game = fixture();

    let before = game.world().save_state();

    let result = game.melee_hit(player_hit());
    assert!(
        matches!(result, Err(Error::Unsupported(_))),
        "an invulnerable target's first hit must be rejected as unsupported, got {result:?}"
    );

    let after = game.world().save_state();

    assert_ne!(
        before, after,
        "save_state() must change once the rejected hit lazily built the \
         target's actor base: the chosen CMB-36 contract persists that \
         entry even though the hit itself never applied"
    );
    assert!(
        after.len() > before.len(),
        "the newly cached actor base must add bytes to save_state() \
         (before {} bytes, after {} bytes)",
        before.len(),
        after.len()
    );

    let reloaded = Game::new({
        let mut order = LoadOrder::new();
        let mut acbs = vec![0; 24];
        acbs[..4].copy_from_slice(&flags::INVULNERABLE.to_le_bytes());
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
        plugin.push_top(weapon(0xA00, 10, 6));
        settings(&mut plugin);
        order.add("Cmb36Lazy.esm", plugin).unwrap();
        order
    });
    reloaded
        .world_mut()
        .load_state(&after)
        .expect("save_state() taken after the rejected hit must load back cleanly");
}

/// Before any hit, no cache has been built yet, so a plain save/load round
/// trip of the untouched fixture must also succeed. This anchors the
/// "before" baseline the persistence assertions above compare against, and
/// separately confirms the empty `actor_bases` map itself round-trips.
#[test]
fn cmb_36_untouched_target_save_state_round_trips() {
    let game = fixture();
    let before = game.world().save_state();

    game.world_mut()
        .load_state(&before)
        .expect("an untouched fixture's save_state() must load back cleanly");
}
