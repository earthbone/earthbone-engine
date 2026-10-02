// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Adversarial coverage for CMB-35's claim that scripted actor-value damage
//! clamps a protected or essential actor at its bleedout threshold instead
//! of killing it (docs/combat.md, "Scripted and magic damage clamp at
//! bleedout, CMB-34 and CMB-35"; `World::clamp_health_at_bleedout`,
//! `crates/game/src/world/bleedout.rs`; `World::change_actor_value`,
//! `crates/game/src/world/actors.rs`).
//!
//! The clamp commits through `bleedout::commit_bleedout_health`, which picks
//! the component that keeps the actor's maximum honest. When the clamped
//! value fits under the maximum it moves only `damage`, the same correction
//! `DamageActorValue` and ordinary weapon damage already relied on. When a
//! decrease left the maximum itself below the threshold, which happens for
//! the four actor-value natives that write `base`, `permanent` or
//! `temporary` directly (`SetActorValue`, `ModActorValue`,
//! `ForceActorValue`, `ModTemporaryActorValue`), it clears `damage` and
//! calls `Values::force` instead, so the committed `current()` health lands
//! exactly at the documented threshold for those natives too, not at
//! whatever value the underlying operation produced.
//!
//! Each test below drives one native far past a protected actor's bleedout
//! threshold and checks the committed health value, not just the
//! `unconscious`/`dead` flags the done check names. `cmb_35_damage_actor_value_...`
//! is the contrast case: the same native family the writer's report and
//! docs/combat.md first proved the mechanism against, included here to show
//! the clamp holds for that path too.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc::flags, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// A protected actor (0x900) with an absolute bleedout threshold of 20,
/// matching combat_bleedout.rs's ACBS layout: flags at bytes 0..4,
/// the absolute threshold override at bytes 22..24.
fn fixture() -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&(flags::PROTECTED | flags::BLEEDOUT_OVERRIDE).to_le_bytes());
    acbs[22..24].copy_from_slice(&20_u16.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Cmb35.esm", plugin).unwrap();
    Game::new(order)
}

/// Baseline health at 100 through `SetActorValue`, the same call the four
/// broken natives below are themselves also tested through, so seeding the
/// fixture never depends on a native this file is trying to falsify.
fn seeded(health: f32) -> Game {
    let game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "health", health)
        .unwrap();
    game
}

/// Reference case: `DamageActorValue` writes through `Values::damage`, so
/// `restore`'s damage-component correction actually lands on the value the
/// operation changed. This is the case docs/combat.md and the writer's
/// report describe, and it survives: health commits exactly at the
/// threshold, not below it.
#[test]
fn cmb_35_damage_actor_value_clamps_committed_health_at_threshold() {
    let game = seeded(100.);
    game.world_mut()
        .damage_actor_value(id(0x900), "health", 500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious, "protected actor must enter bleedout");
    assert!(!actor.dead, "protected actor must not die");
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// `ModActorValue` writes through `Values::modify`, into the `permanent`
/// component. That takes the actor's maximum below the threshold, so
/// `commit_bleedout_health` falls back to its force path: it clears
/// `damage` and calls `Values::force("health", threshold)`, landing the
/// committed health exactly at the threshold instead of at whatever
/// `permanent -= 500` produced.
#[test]
fn cmb_35_mod_actor_value_clamps_committed_health_at_threshold() {
    let game = seeded(100.);
    game.world_mut()
        .mod_actor_value(id(0x900), "health", -500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious, "protected actor must enter bleedout");
    assert!(!actor.dead, "protected actor must not die");
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// `SetActorValue` writes `base` directly. Same force-path commit as
/// `ModActorValue`: the maximum falls below the threshold, so
/// `commit_bleedout_health` clears `damage` and forces health to the
/// threshold.
#[test]
fn cmb_35_set_actor_value_clamps_committed_health_at_threshold() {
    let game = seeded(100.);
    game.world_mut()
        .set_actor_value(id(0x900), "health", -500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious);
    assert!(!actor.dead);
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// `ForceActorValue` writes `permanent` to make `current()` equal the
/// target value directly, the native with the least room to spare since the
/// caller needs no large delta, just a negative absolute value. The force
/// path still lands committed health exactly at the threshold.
#[test]
fn cmb_35_force_actor_value_clamps_committed_health_at_threshold() {
    let game = seeded(100.);
    game.world_mut()
        .force_actor_value(id(0x900), "health", -500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious);
    assert!(!actor.dead);
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// `ModTemporaryActorValue` writes `temporary`, the component active magic
/// effects use for non-recover contributions. Same force-path commit again.
#[test]
fn cmb_35_mod_temporary_actor_value_clamps_committed_health_at_threshold() {
    let game = seeded(100.);
    game.world_mut()
        .mod_temporary_actor_value(id(0x900), "health", -500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious);
    assert!(!actor.dead);
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// Open question 3 in the task pack asked whether a `validate_combat_actor`
/// style gate is needed for an already-unconscious actor, or whether the
/// clamp's own idempotency covers it. `commit_bleedout_health`'s force path
/// answers this: each repeated `ModActorValue` call still drives
/// `permanent` further down, but the clamp recomputes the threshold and
/// forces health back to it on every call, so three repeated -100 calls
/// after the actor is already unconscious hold committed health at 20, not
/// at some ever falling value.
#[test]
fn cmb_35_mod_actor_value_committed_health_holds_at_threshold_on_repeated_calls_while_unconscious()
{
    let game = seeded(100.);
    let mut world = game.world_mut();
    world.mod_actor_value(id(0x900), "health", -500.).unwrap();
    assert!(world.actor(id(0x900)).unwrap().unconscious);
    for _ in 0..3 {
        world.mod_actor_value(id(0x900), "health", -100.).unwrap();
        assert!(
            world.actor(id(0x900)).unwrap().unconscious,
            "still in bleedout after every repeated call"
        );
        assert!(!world.actor(id(0x900)).unwrap().dead);
    }
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}

/// Same clamp reached through an essential actor rather than a merely
/// protected one, since `change_actor_value` grants no player exception and
/// treats both flags identically (`protected || essential`).
#[test]
fn cmb_35_mod_actor_value_clamps_essential_actor_too() {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&(flags::ESSENTIAL | flags::BLEEDOUT_OVERRIDE).to_le_bytes());
    acbs[22..24].copy_from_slice(&20_u16.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Cmb35Essential.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 100.)
        .unwrap();

    game.world_mut()
        .mod_actor_value(id(0x900), "health", -500.)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert!(actor.unconscious);
    assert!(!actor.dead);
    drop(world);
    close(game.world_mut().actor_value(id(0x900), "health"), 20.);
}
