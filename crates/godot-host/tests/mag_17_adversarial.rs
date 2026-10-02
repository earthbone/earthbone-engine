// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../game/tests/common/mag_adversarial.rs"]
mod common;

use common::{effect, game, id, spell, ACTOR, SPELL};

#[test]
fn mag_17_location_cast_rejects_non_finite_selected_positions_atomically() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, 500.0, 100)],
        &[(ACTOR, [0.0; 3])],
    );
    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(ACTOR),
        Some(id(ACTOR)),
        [f32::NAN, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(result.is_err(), "a selected location must be finite");
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_aimed_delivery_before_applying_effects() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 2, 500.0, 100)],
        &[(ACTOR, [0.0; 3])],
    );
    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(ACTOR),
        Some(id(ACTOR)),
        [10.0, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "an aimed spell is not a target-location cast"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}
