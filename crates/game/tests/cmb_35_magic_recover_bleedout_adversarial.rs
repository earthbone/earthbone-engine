// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-35's magic-damage path
//! (`World::apply_effect_values`, `crates/game/src/world/magic.rs`), the
//! same bleedout clamp covered against the actor-value natives in
//! `cmb_35_script_damage_bleedout_adversarial.rs`, reached here through a
//! cast spell instead.
//!
//! `apply_effect_values` computes the health delta through `change_values`
//! (magic.rs). For a RECOVER-flagged effect, `change_values` writes through
//! `Values::modify_temporary`, into the `temporary` component, not
//! `damage`. The clamp commit that follows,
//! `bleedout::commit_bleedout_health`, handles that: when the decrease
//! takes the actor's maximum below the threshold, which a RECOVER effect
//! writing into `temporary` does, the helper clears `damage` and calls
//! `Values::force` instead of relying on `damage` alone, so committed
//! health lands exactly at the threshold. `apply_effect_values` also gates
//! its death check on `entering_bleedout` rather than reading `current()`
//! fresh, so a protected actor that enters bleedout is never also killed by
//! the same call. A RECOVER effect flagged DETRIMENTAL (magnitude negated,
//! see `magic.rs`'s `signed` conversion) reaches exactly this path: a
//! protected actor hit with one carries `unconscious = true`, skips death,
//! and commits health at the threshold.
//!
//! The effect and spell records below are minimal DATA/SPIT/EFID/EFIT
//! fixtures with no VMAD script binding, since the clamp commit this file
//! tests runs before any Papyrus event is delivered.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, npc::flags};

// Mirrors magic.rs's private flag constants (crates/game/src/world/magic.rs):
// HOSTILE = 1, RECOVER = 1 << 1, DETRIMENTAL = 1 << 2.
const RECOVER: u32 = 1 << 1;
const DETRIMENTAL: u32 = 1 << 2;

const TARGET_BASE: u32 = 0x800;
const TARGET: u32 = 0x900;
const CASTER_BASE: u32 = 0x801;
const CASTER: u32 = 0x901;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// A Value Modifier (archetype 0) health effect, delivered at a target
/// actor (delivery 3), non-constant (cast_type 1) so it lines up with
/// `spell()`'s own cast_type below. `effect_flags` carries RECOVER and
/// DETRIMENTAL for the tests that need them.
fn effect(effect_flags: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, effect_flags);
    put(&mut data, 64, 0); // archetype: Value Modifier
    put(&mut data, 68, 24); // primary_actor_value: Health
    put(&mut data, 80, 1); // cast_type, matches the spell's own cast_type
    put(&mut data, 84, 3); // delivery: Target Actor
    Record::new(Tag(*b"MGEF"), EFFECT).sub(Tag(*b"DATA"), data)
}

/// An instant (duration 0) spell with the given magnitude, matching
/// `effect()`'s cast_type and delivery exactly, the pairing
/// `World::cast_spell` requires between a spell and its effects.
fn spell(magnitude: f32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, 1); // cast_type
    put(&mut data, 20, 3); // delivery: Target Actor
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&0u32.to_le_bytes());
    Record::new(Tag(*b"SPEL"), SPELL)
        .sub(Tag(*b"SPIT"), data)
        .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
        .sub(Tag(*b"EFIT"), efit)
}

/// A protected target (0x900) with an absolute bleedout threshold of 20 and
/// an NPC caster (0x901), so the caster is never the player and
/// `apply_effect_values`'s `player_exception` never applies.
fn fixture(target_flags: u32, effect_flags: u32) -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&(target_flags | flags::BLEEDOUT_OVERRIDE).to_le_bytes());
    acbs[22..24].copy_from_slice(&20_u16.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), TARGET_BASE).sub(Tag(*b"ACBS"), acbs));
    plugin
        .push_top(Record::new(Tag(*b"ACHR"), TARGET).sub(Tag(*b"NAME"), TARGET_BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), CASTER_BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin
        .push_top(Record::new(Tag(*b"ACHR"), CASTER).sub(Tag(*b"NAME"), CASTER_BASE.to_le_bytes()));
    plugin.push_top(effect(effect_flags));
    plugin.push_top(spell(500.));
    let mut order = LoadOrder::new();
    order.add("Cmb35Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.)
        .unwrap();
    game
}

/// Reference case: a DETRIMENTAL effect without RECOVER writes through
/// `Values::damage` in `change_values` (`delta < 0.0` branch), so the
/// clamp's `restore` call lands on the component that actually changed.
/// Health commits exactly at the threshold.
#[test]
fn cmb_35_detrimental_only_magic_effect_clamps_committed_health_at_threshold() {
    let mut game = fixture(flags::PROTECTED, DETRIMENTAL);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(TARGET)).unwrap();
    assert!(actor.unconscious, "protected target must enter bleedout");
    assert!(!actor.dead, "protected target must not die");
    drop(world);
    close(game.world_mut().actor_value(id(TARGET), "health"), 20.);
}

/// A RECOVER|DETRIMENTAL effect writes through `Values::modify_temporary`
/// in `change_values` instead of `Values::damage`. That takes the actor's
/// maximum below the threshold, so `commit_bleedout_health` takes its force
/// path: it clears `damage` and calls `Values::force`, landing committed
/// health exactly at the threshold.
///
/// The death check is gated on `entering_bleedout`, computed before the
/// commit above, rather than on a fresh read of `values.get("health").current()`.
/// That ordering is what keeps a protected, non-essential target hit by a
/// non-player caster from being killed by the same call that just flagged
/// it as entering bleedout: `dies` stays `false` whenever `entering_bleedout`
/// is `true`, regardless of what `current()` would have read on its own.
#[test]
fn cmb_35_recover_detrimental_magic_effect_clamps_protected_actor_instead_of_killing() {
    let mut game = fixture(flags::PROTECTED, RECOVER | DETRIMENTAL);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(TARGET)).unwrap();
    assert!(actor.unconscious, "protected target must enter bleedout");
    assert!(!actor.dead, "protected target must not die");
    drop(world);
    close(game.world_mut().actor_value(id(TARGET), "health"), 20.);
}
