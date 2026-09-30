// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial engine-save checks for distinct actor-value modifier components.

use soe_esm::LoadOrder;
use soe_game::Game;

fn fresh() -> (Game, soe_esm::GlobalFormId) {
    let mut game = Game::new(LoadOrder::new());
    let actor = game.player();
    (game, actor)
}

#[test]
fn equal_totals_keep_permanent_and_temporary_splits_after_reload() {
    let (original, actor) = fresh();
    let mut world = original.world_mut();
    world.mod_actor_value(actor, "health", 17.25).unwrap();
    world
        .mod_temporary_actor_value(actor, "health", 4.5)
        .unwrap();
    world.damage_actor_value(actor, "health", 9.75).unwrap();
    world.mod_actor_value(actor, "magicka", 4.5).unwrap();
    world
        .mod_temporary_actor_value(actor, "magicka", 17.25)
        .unwrap();
    world.damage_actor_value(actor, "magicka", 9.75).unwrap();
    assert_eq!(
        world.actor_value(actor, "health"),
        world.actor_value(actor, "magicka")
    );
    assert_eq!(
        world.actor_value_maximum(actor, "health"),
        world.actor_value_maximum(actor, "magicka")
    );
    drop(world);

    let bytes = original.save_state().unwrap();
    let (mut restored, _) = fresh();
    restored.load_state(&bytes).unwrap();
    restored.load_state(&bytes).unwrap();

    let mut world = restored.world_mut();
    assert_eq!(
        world.actor_value(actor, "health"),
        world.actor_value(actor, "magicka")
    );
    world
        .mod_temporary_actor_value(actor, "health", -4.5)
        .unwrap();
    world
        .mod_temporary_actor_value(actor, "magicka", -17.25)
        .unwrap();
    assert_eq!(world.actor_value_maximum(actor, "health"), 117.25);
    assert_eq!(world.actor_value_maximum(actor, "magicka"), 104.5);
    assert_eq!(world.actor_value(actor, "health"), 107.5);
    assert_eq!(world.actor_value(actor, "magicka"), 94.75);
}

#[test]
fn damage_survives_reload_without_becoming_permanent_or_temporary() {
    let (original, actor) = fresh();
    original
        .world_mut()
        .mod_actor_value(actor, "health", 13.0)
        .unwrap();
    original
        .world_mut()
        .mod_temporary_actor_value(actor, "health", 7.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(actor, "health", 11.0)
        .unwrap();
    let bytes = original.save_state().unwrap();

    let (mut restored, _) = fresh();
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.world_mut().actor_value_maximum(actor, "health"),
        120.0
    );
    assert_eq!(restored.world_mut().actor_value(actor, "health"), 109.0);
    restored
        .world_mut()
        .restore_actor_value(actor, "health", 6.0)
        .unwrap();
    assert_eq!(
        restored.world_mut().actor_value_maximum(actor, "health"),
        120.0
    );
    assert_eq!(restored.world_mut().actor_value(actor, "health"), 115.0);
}
