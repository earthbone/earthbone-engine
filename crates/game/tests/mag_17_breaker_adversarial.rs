// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mag_adversarial.rs"]
mod support;
use support::*;

#[test]
fn mag_17_location_cast_accepts_a_point_exactly_at_authored_range() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 20.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0]), (OTHER, [12.0, 16.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [12.0, 16.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_ok(),
        "the 3D endpoint at the range limit is valid"
    );
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 0.0);
}

#[test]
fn mag_17_location_cast_rejects_a_missing_spell_without_mutation() {
    let mut game = game(effect(5, -1), vec![], &[(SOURCE, [0.0, 0.0, 0.0])]);

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [0.0, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(result.is_err(), "an unresolved spell cannot be delivered");
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_point_past_authored_range_atomically() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 20.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );
    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [20.01, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    assert!(result.is_err());
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}
