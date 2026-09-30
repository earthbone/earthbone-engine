// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-06 engine-save checks for separated current and permanent actor values.

use soe_esm::LoadOrder;
use soe_game::Game;

fn fresh() -> (Game, soe_esm::GlobalFormId) {
    let mut game = Game::new(LoadOrder::new());
    let actor = game.player();
    (game, actor)
}

#[test]
fn sav_06_current_actor_value_survives_engine_save_load() {
    let (original, actor) = fresh();
    original
        .world_mut()
        .mod_actor_value(actor, "health", 20.0)
        .unwrap();
    original
        .world_mut()
        .mod_temporary_actor_value(actor, "health", 30.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(actor, "health", 40.0)
        .unwrap();
    let bytes = original.save_state().unwrap();

    let (mut restored, _) = fresh();
    restored.load_state(&bytes).unwrap();

    assert_eq!(restored.world_mut().actor_value(actor, "health"), 110.0);
}

#[test]
fn sav_06_permanent_modifier_remains_separate_after_engine_save_load() {
    let (original, actor) = fresh();
    original
        .world_mut()
        .mod_actor_value(actor, "health", 20.0)
        .unwrap();
    original
        .world_mut()
        .mod_temporary_actor_value(actor, "health", 30.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(actor, "health", 40.0)
        .unwrap();
    let bytes = original.save_state().unwrap();

    let (mut restored, _) = fresh();
    restored.load_state(&bytes).unwrap();

    assert_eq!(
        restored.world_mut().actor_value_maximum(actor, "health"),
        150.0
    );
    assert_eq!(
        restored.world_mut().base_actor_value(actor, "health"),
        100.0
    );
    restored
        .world_mut()
        .mod_temporary_actor_value(actor, "health", -30.0)
        .unwrap();
    restored
        .world_mut()
        .restore_actor_value(actor, "health", 40.0)
        .unwrap();
    assert_eq!(restored.world_mut().actor_value(actor, "health"), 120.0);
    assert_eq!(
        restored.world_mut().actor_value_maximum(actor, "health"),
        120.0
    );
}
