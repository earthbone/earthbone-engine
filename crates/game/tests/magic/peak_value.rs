// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const OTHER_KEYWORD: u32 = 0x401;
const OTHER_EFFECT: u32 = 0x201;

fn peak_effect(form: u32, keyword: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, 1 << 1);
    put(&mut data, 8, keyword);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 34);
    put(&mut data, 68, 24);
    put(&mut data, 72, 25);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    data[76..80].copy_from_slice(&100.0_f32.to_le_bytes());
    Record::new(T(b"MGEF"), form)
        .zsub(T(b"EDID"), "PeakHealth")
        .sub(T(b"DATA"), data)
        .sub(T(b"KWDA"), keyword.to_le_bytes())
}

fn peak_plugin() -> Plugin {
    let mut order = plugin(
        peak_effect(EFFECT, KEYWORD),
        vec![
            spell(SPELL, 0, false, 25.0, 5),
            spell(0x302, 0, false, 10.0, 10),
        ],
    );
    let mut distinct_keyword_spell = spell(0x303, 0, false, 5.0, 10);
    distinct_keyword_spell.subrecords[1].data = OTHER_EFFECT.to_le_bytes().to_vec();
    order.push_top(Record::new(T(b"KYWD"), OTHER_KEYWORD));
    order.push_top(peak_effect(OTHER_EFFECT, OTHER_KEYWORD));
    order.push_top(distinct_keyword_spell);
    order
}

fn cast(game: &mut Game, spell: u32) {
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(SOURCE),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_55_peak_value_modifier_uses_the_largest_active_group_value() {
    let mut game = game(peak_plugin());
    cast(&mut game, SPELL);
    cast(&mut game, 0x302);

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 125.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
}

#[test]
fn mag_55_peak_value_modifier_recomputes_when_the_peak_expires() {
    let mut game = game(peak_plugin());
    cast(&mut game, SPELL);
    cast(&mut game, 0x302);

    game.tick(5.0);

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    assert_eq!(game.world().active_effects().count(), 1);
}

#[test]
fn mag_55_peak_value_modifier_competes_only_with_its_keyword_group() {
    let mut game = game(peak_plugin());
    cast(&mut game, SPELL);
    cast(&mut game, 0x303);

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 130.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
}

#[test]
fn peak_value_without_related_keyword_rejects_as_unsupported_without_mutation() {
    let mut game = game(plugin(
        peak_effect(EFFECT, 0),
        vec![spell(SPELL, 0, false, 25.0, 5)],
    ));
    let before = game.save_state().unwrap();
    let error = game
        .shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MagicError::Unsupported("peak value keyword")
    ));
    assert_eq!(game.save_state().unwrap(), before);
}
