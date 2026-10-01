// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn mag_73_interrupt_cast_stops_fire_and_forget_charge_without_effect_or_cost() {
    let mut game = game(plugin(
        effect(0, 0, 24, false),
        vec![spell(SPELL, 1, false, 20.0, 5)],
    ));
    game.world_mut()
        .begin_spell_cast(id(ACTOR), id(SPELL), Some(id(OTHER)))
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);

    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    native(&mut game, actor, "InterruptCast", &[]);

    assert_eq!(game.world().pending_spell_cast(id(ACTOR)), None);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
}

#[test]
fn mag_73_interrupt_cast_stops_concentration_without_effect_or_cost() {
    let mut game = game(plugin(
        effect(0, 0, 24, false),
        vec![spell(SPELL, 2, false, 20.0, 5)],
    ));
    game.world_mut()
        .begin_spell_cast(id(ACTOR), id(SPELL), Some(id(OTHER)))
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);

    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    native(&mut game, actor, "InterruptCast", &[]);

    assert_eq!(game.world().pending_spell_cast(id(ACTOR)), None);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
}

#[test]
fn mag_73_interrupt_cast_does_not_revoke_released_fire_and_forget_effect() {
    let mut game = game(plugin(
        effect(0, 0, 24, false),
        vec![spell(SPELL, 1, false, 20.0, 5)],
    ));
    game.world_mut()
        .begin_spell_cast(id(ACTOR), id(SPELL), Some(id(OTHER)))
        .unwrap();
    let shared = game.shared_world();
    let effects = shared
        .borrow_mut()
        .release_spell_cast(id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    assert_eq!(effects.len(), 1);
    assert_eq!(game.world().active_effects().count(), 1);

    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    native(&mut game, actor, "InterruptCast", &[]);

    assert_eq!(game.world().pending_spell_cast(id(ACTOR)), None);
    assert_eq!(game.world().active_effects().count(), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
}
