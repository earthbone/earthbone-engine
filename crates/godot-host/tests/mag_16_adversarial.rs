// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-16 boundary checks for the form-backed gate used by the host touch path.

#[path = "../../game/tests/common/mag_adversarial.rs"]
mod common;

use common::{cast, effect, game, id, spell, ACTOR, OTHER, SPELL};
use soe_game::world::magic::MagicError;

#[test]
fn mag_16_zero_reach_allows_only_a_target_at_the_source_position() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 1, 0.0, 0)],
        &[(ACTOR, [0.0; 3]), (OTHER, [0.0; 3])],
    );

    assert!(cast(&mut game, SPELL, ACTOR, OTHER).is_ok());
}

#[test]
fn mag_16_zero_reach_rejects_a_distinct_target_without_effects() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 1, 0.0, 0)],
        &[(ACTOR, [0.0; 3]), (OTHER, [0.01, 0.0, 0.0])],
    );

    assert!(matches!(
        cast(&mut game, SPELL, ACTOR, OTHER),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_16_target_actor_delivery_does_not_use_touch_reach() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 3, 0.0, 0)],
        &[(ACTOR, [0.0; 3]), (OTHER, [10000.0, 0.0, 0.0])],
    );

    assert!(cast(&mut game, SPELL, ACTOR, OTHER).is_ok());
}

#[test]
fn mag_16_touch_rejects_a_missing_target_reference() {
    let mut game = game(effect(0, 0), vec![spell(SPELL, 1, 1, 100.0, 0)], &[]);

    assert!(matches!(
        cast(&mut game, SPELL, ACTOR, 0xdead),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}
