// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::mag_adversarial::{cast, effect, game, id, spell, ACTOR, EFFECT, OTHER, SPELL};
use soe_game::world::magic::MagicError;

#[test]
fn mag_16_touch_rejects_a_non_actor_reference_without_applying_effects() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 1, 10.0, 0)],
        &[(ACTOR, [0.0; 3]), (OTHER, [1.0, 0.0, 0.0])],
    );
    // MGEF exists but is not an eligible actor reference.
    let result = cast(&mut game, SPELL, ACTOR, EFFECT);
    assert!(matches!(result, Err(MagicError::Target)));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}
