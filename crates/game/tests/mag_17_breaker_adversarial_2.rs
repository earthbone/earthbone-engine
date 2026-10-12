// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::mag_adversarial::*;

#[test]
fn mag_17_location_cast_rejects_non_finite_positions_atomically() {
    for position in [[f32::NAN, 0.0, 0.0], [0.0, f32::INFINITY, 0.0]] {
        let mut game = game(
            effect(5, -1),
            vec![spell(SPELL, 1, 4, 20.0, 0)],
            &[(SOURCE, [0.0, 0.0, 0.0])],
        );

        let result = game.shared_world().borrow_mut().cast_spell_at_location(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            position,
            true,
            &mut |name, class| game.vm.create_object_of(name, class),
        );

        assert!(result.is_err(), "non-finite selected positions are invalid");
        assert_eq!(game.world().active_effects().count(), 0);
        assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
    }
}

#[test]
fn mag_17_location_cast_rejects_missing_source_without_mutation() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 20.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(0x999),
        Some(id(SOURCE)),
        [0.0, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "a location cast needs a live source reference"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_other_delivery_modes_without_mutation() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 3, 20.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [0.0, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "only Target Location delivery accepts a position"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_obstruction_without_mutation() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 20.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [10.0, 0.0, 0.0],
        false,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "an obstructed location cannot receive the cast"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_uses_three_dimensional_range() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 1.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [15.1, 15.1, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "the diagonal point is beyond one foot in 3D"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}
