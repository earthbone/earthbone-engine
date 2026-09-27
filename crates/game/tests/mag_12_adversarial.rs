// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "common/mag_adversarial.rs"]
mod support;
use support::*;

#[test]
fn independently_capped_resistances_reduce_hostile_magnitude_multiplicatively() {
    let mut game = game(effect(5, 41), vec![spell(SPELL, 1, 0, 0.0, 0)], &[]);
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistmagic", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistfire", 100.0)
        .unwrap();
    cast(&mut game, SPELL, ACTOR, ACTOR).unwrap();
    let health = game.world_mut().actor_value(id(ACTOR), "health");
    assert!(
        (health - 97.75).abs() < 0.001,
        "health after two capped resistances: {health}"
    );
}
