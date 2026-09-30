// SPDX-License-Identifier: GPL-3.0-or-later

//! MAG-29 Calm aggression and combat suspension.

use super::*;

const RECOVER: u32 = 1 << 1;

fn calm_game() -> Game {
    game(plugin(
        effect(RECOVER | (1 << 2), 6, 24, false),
        vec![spell(SPELL, 0, false, 1.0, 2)],
    ))
}

#[test]
fn mag_29_calm_uses_aggression_and_suspends_it_for_the_effect_duration() {
    let mut game = calm_game();
    game.world_mut()
        .set_actor_value(id(OTHER), "aggression", 1.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();

    assert_eq!(game.world_mut().actor_value(id(OTHER), "aggression"), 0.0);
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "aggression"), 0.0);
}

#[test]
fn mag_29_calm_drops_combat_and_restores_aggression_when_it_expires() {
    let mut game = calm_game();
    game.world_mut()
        .set_actor_value(id(OTHER), "aggression", 1.0)
        .unwrap();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(OTHER)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(ACTOR));
    }

    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick(0.1);

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(OTHER)).unwrap();
        assert!(!actor.in_combat);
        assert_eq!(actor.combat_target, None);
    }

    game.tick(1.9);

    assert_eq!(game.world_mut().actor_value(id(OTHER), "aggression"), 1.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
