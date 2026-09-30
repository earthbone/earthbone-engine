// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn conditional_spell(concentration: bool) -> Record {
    let mut record = spell(SPELL, 0, false, 20.0, 5);
    let spit = record
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .expect("spell fixture has SPIT");
    put(
        spit.data.as_mut_slice(),
        16,
        if concentration { 2 } else { 1 },
    );
    put(spit.data.as_mut_slice(), 20, 3);
    record.sub(T(b"CTDA"), condition(14, 26, 100.0, 0))
}

fn conditional_effect(concentration: bool) -> Record {
    let mut record = effect(4, 0, 24, false);
    let data = record
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .expect("effect fixture has DATA");
    put(
        data.data.as_mut_slice(),
        80,
        if concentration { 2 } else { 1 },
    );
    put(data.data.as_mut_slice(), 84, 3);
    record.sub(T(b"CTDA"), condition(14, 24, 100.0, 0))
}

fn conditioned_game(concentration: bool) -> Game {
    let game = game(plugin(
        conditional_effect(concentration),
        vec![conditional_spell(concentration)],
    ));
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.0)
            .unwrap();
    }
    game
}

#[test]
fn mag_70_timed_spell_rechecks_conditions_at_each_second() {
    let mut game = conditioned_game(false);
    let applied = cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(applied.len(), 1);
    game.tick(0.5);
    let before = game.world_mut().actor_value(id(OTHER), "health");
    assert!(before < 100.0);

    game.world_mut()
        .set_actor_value(id(OTHER), "stamina", 80.0)
        .unwrap();
    game.tick(0.49);
    assert_eq!(game.world().active_effects().count(), 1);
    let before_check = game.world_mut().actor_value(id(OTHER), "health");
    assert!(before_check < before);

    game.tick(0.01);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
}

#[test]
fn mag_70_concentration_rechecks_conditions_for_each_reapplication() {
    let mut game = conditioned_game(true);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(OTHER)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    game.tick(0.1);
    let after_first_slice = game.world_mut().actor_value(id(OTHER), "health");
    assert!(after_first_slice < 100.0);
    game.world_mut()
        .set_actor_value(id(OTHER), "stamina", 80.0)
        .unwrap();
    game.tick(0.1);

    assert_eq!(
        game.world_mut().actor_value(id(OTHER), "health"),
        after_first_slice
    );
    assert!(game.world().is_concentration_casting(id(ACTOR)));
}

#[test]
fn mag_70_save_load_retains_fractional_time_until_condition_check() {
    let mut original = conditioned_game(false);
    cast(&mut original, SPELL, OTHER).unwrap();
    original.tick(0.5);
    let saved = original.save_state().unwrap();

    let mut restored = conditioned_game(false);
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world().active_effects().next().unwrap().elapsed,
        0.5
    );
    restored
        .world_mut()
        .set_actor_value(id(OTHER), "stamina", 80.0)
        .unwrap();
    restored.tick(0.49);
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(0.01);
    assert_eq!(restored.world().active_effects().count(), 0);
}
