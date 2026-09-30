// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn mag_34_invisibility_is_active_and_expires_with_its_spell() {
    let mut game = game(plugin(
        effect(0, 11, 0, false),
        vec![spell(SPELL, 0, false, 1.0, 4)],
    ));
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert!(game.world().is_invisible(id(ACTOR)));
    game.tick(4.0);
    assert!(!game.world().is_invisible(id(ACTOR)));
}

#[test]
fn mag_34_invisibility_ends_when_actor_casts_another_spell() {
    let mut records = plugin(
        effect(0, 11, 0, false),
        vec![
            spell(SPELL, 0, false, 1.0, 20),
            spell_with_base_effect(0x302, 0, false, 1.0, 0, 0, 0x201),
        ],
    );
    records.push_top(effect_with_id(0x201, 0, 0, 24, false, 0, &[KEYWORD]));
    let mut game = game(records);
    let shared = game.shared_world();
    for spell_id in [SPELL, 0x302] {
        shared
            .borrow_mut()
            .cast_spell(
                id(spell_id),
                id(SOURCE),
                Some(id(ACTOR)),
                Some(id(ACTOR)),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
    }

    assert!(!game.world().is_invisible(id(ACTOR)));
}

#[test]
fn mag_34_invisibility_ends_when_actor_activates_a_reference() {
    let mut game = game(plugin(
        effect(0, 11, 0, false),
        vec![spell(SPELL, 0, false, 1.0, 20)],
    ));
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(game.world().is_invisible(id(ACTOR)));

    assert!(game.activate(id(0x701), id(ACTOR)));
    assert!(!game.world().is_invisible(id(ACTOR)));
}
