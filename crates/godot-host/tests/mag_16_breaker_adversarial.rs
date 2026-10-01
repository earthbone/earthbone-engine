// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../game/tests/common/mag_adversarial.rs"]
mod common;

use common::{cast, effect, game, id, spell, ACTOR, OTHER, SPELL};
use soe_game::world::magic::MagicError;

#[test]
fn mag_16_touch_cast_rejects_a_target_just_beyond_authored_reach() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 1, 10.0, 0)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [10.001, 0.0, 0.0])],
    );
    assert!(matches!(
        cast(&mut game, SPELL, ACTOR, OTHER),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn mag_16_touch_cast_rejects_an_aimed_spell_without_effects() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 2, 100.0, 0)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [1.0, 0.0, 0.0])],
    );
    assert!(matches!(
        cast(&mut game, SPELL, ACTOR, OTHER),
        Err(MagicError::Unsupported(_))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}
