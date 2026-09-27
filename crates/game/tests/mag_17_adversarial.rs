// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "common/mag_adversarial.rs"] mod support;
use support::*;

#[test]
fn location_area_hits_only_actors_around_the_selected_point() {
    let mut game = game(effect(5, -1), vec![spell(SPELL, 1, 4, 500.0, 100)], &[
        (SOURCE, [0.0, 0.0, 0.0]), (ACTOR, [100.0, 0.0, 0.0]), (OTHER, [201.0, 0.0, 0.0])]);
    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL), id(SOURCE), Some(id(SOURCE)), [100.0, 0.0, 0.0], true, &mut |name, class| game.vm.create_object_of(name, class));
    assert_eq!(result.unwrap().len(), 2);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 0.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 0.0);
}

#[test]
fn obstructed_location_cast_does_not_apply_or_leave_effects() {
    let mut game = game(effect(5, -1), vec![spell(SPELL, 1, 4, 500.0, 0)], &[]);
    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL), id(SOURCE), Some(id(SOURCE)), [10.0, 0.0, 0.0], false, &mut |name, class| game.vm.create_object_of(name, class));
    assert!(result.is_err());
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
