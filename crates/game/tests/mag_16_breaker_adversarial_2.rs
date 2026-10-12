// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::mag_adversarial::*;

#[test]
fn mag_16_touch_cast_accepts_a_target_exactly_at_authored_reach() {
    let mut game = game(
        effect(0, 0),
        vec![spell(SPELL, 1, 1, 10.0, 0)],
        &[(ACTOR, [0.0; 3]), (OTHER, [10.0, 0.0, 0.0])],
    );

    let applied = cast(&mut game, SPELL, ACTOR, OTHER).unwrap();

    assert_eq!(applied.len(), 1);
    assert_eq!(game.world().active_effects().count(), 1);
}
