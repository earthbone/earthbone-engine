// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "common/mag_adversarial.rs"] mod support;
use support::*;

#[test]
fn aimed_contact_hits_the_resolved_actor_once_and_preserves_attribution() {
    let mut game = game(effect(5, -1), vec![spell(SPELL, 1, 2, 0.0, 0)], &[]);
    let result = game.shared_world().borrow_mut().cast_spell_on_projectile_contact(
        id(SPELL), id(SOURCE), Some(id(SOURCE)), id(OTHER), &mut |name, class| game.vm.create_object_of(name, class));
    assert_eq!(result.unwrap().len(), 1);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 0.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    let world = game.world();
    let active = world.active_effects().next().unwrap();
    assert_eq!((active.spell, active.source, active.caster, active.target), (id(SPELL), id(SOURCE), Some(id(SOURCE)), id(OTHER)));
}
