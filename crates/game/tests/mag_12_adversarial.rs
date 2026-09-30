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

#[test]
fn spell_ignore_resistance_preserves_hostile_magnitude() {
    let mut definition = spell(SPELL, 1, 0, 0.0, 0);
    let spit = definition
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .unwrap();
    put(&mut spit.data, 4, 1 << 20);
    let mut game = game(effect(5, 41), vec![definition], &[]);
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistmagic", 85.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistfire", 85.0)
        .unwrap();
    cast(&mut game, SPELL, ACTOR, ACTOR).unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 0.0);
}

#[test]
fn location_area_uses_each_recipients_resistance() {
    let positions = &[
        (SOURCE, [0.0, 0.0, 0.0]),
        (ACTOR, [10.0, 0.0, 0.0]),
        (OTHER, [12.0, 0.0, 0.0]),
    ];
    let mut game = game(effect(5, 41), vec![spell(SPELL, 1, 4, 100.0, 5)], positions);
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistmagic", 85.0)
        .unwrap();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell_at_location(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            [11.0, 0.0, 0.0],
            true,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 85.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 0.0);
}
