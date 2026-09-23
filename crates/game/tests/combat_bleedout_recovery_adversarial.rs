// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-13 entry and recovery
//! (`crates/game/src/world/bleedout.rs`, `crates/game/src/world/combat.rs`).
//!
//! `World::resolve_bleedout_recovery`'s documented contract (docs/combat.md,
//! "Bleedout entry and recovery, CMB-13") is: a health change that carries
//! current health back above the cached threshold clears `unconscious`,
//! unless `SetNoBleedoutRecovery` holds the actor down. Recovery does not
//! depend on the flags that caused entry.
//!
//! The first implementation re-read `protected` and `essential` at recovery
//! time and returned early when both were false. A script that lifted
//! essential status on a target already in bleedout, through
//! `World::set_actor_base_essential` (`crates/game/src/world/death.rs`), the
//! function the `SetEssential` Papyrus native calls, left that target
//! unconscious for good. These tests found that defect and now guard the fix.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, npc::flags, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Mirrors `combat_bleedout_recovery.rs`'s weapon fixture exactly, so the
/// same health and damage numbers that file already verified apply here.
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

fn fixture(target_flags: u32) -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&target_flags.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Bleedout.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target: id(0x900),
        weapon: id(0xA00),
    }
}

/// A target that entered bleedout as essential, then has essential lifted by
/// a script (`SetEssential(false)`, the common "scene is over" pattern) while
/// still unconscious, must still recover on the next health restore that
/// clears the threshold. The recovery contract names only the threshold
/// comparison and `NoBleedoutRecovery`, not the flag that caused entry.
#[test]
fn cmb_13_essential_lifted_while_unconscious_must_not_strand_recovery() {
    let mut game = fixture(flags::ESSENTIAL);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(hit()).unwrap();
    assert!(!result.killed, "essential target must not die");
    assert!(
        game.world_mut().actor(id(0x900)).unwrap().unconscious,
        "target must enter bleedout"
    );

    // The scene-ending pattern: a script lifts essential status on a target
    // that is already down, through the same World::set_actor_base_essential
    // the SetEssential Papyrus native calls.
    game.world_mut().set_actor_base_essential(id(0x800), false);
    assert!(
        !game.world_mut().actor(id(0x900)).unwrap().essential,
        "essential must actually be lifted for this fixture to test the right path"
    );
    assert!(
        !game.world_mut().actor(id(0x900)).unwrap().protected,
        "this fixture never set protected, so both flags read false here"
    );

    // No SetNoBleedoutRecovery was ever called. A full heal must clear
    // unconscious per the documented contract.
    game.world_mut()
        .restore_actor_value(id(0x900), "health", 999.)
        .unwrap();
    assert!(
        !game.world_mut().actor(id(0x900)).unwrap().unconscious,
        "a full heal must recover the target after essential status changed mid-bleedout"
    );
}

/// Same setup with repeated restores, so a recovery that only works on the
/// first health change after the flag change would still fail here.
#[test]
fn cmb_13_stranded_target_never_recovers_on_any_later_heal() {
    let mut game = fixture(flags::ESSENTIAL);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();
    game.melee_hit(hit()).unwrap();
    game.world_mut().set_actor_base_essential(id(0x800), false);

    for _ in 0..3 {
        game.world_mut()
            .restore_actor_value(id(0x900), "health", 1000.)
            .unwrap();
    }
    assert!(
        !game.world_mut().actor(id(0x900)).unwrap().unconscious,
        "repeated full heals must recover a target with no NoBleedoutRecovery set"
    );
}
