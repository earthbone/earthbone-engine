// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/magic_adversarial.rs"]
mod support;

use soe_esm::{Record, Tag};
use support::*;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SPELL: u32 = 0x310;
const EFFECT: u32 = 0x300;

fn condition(function: u16, parameter: u32, expected: f32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data
}

fn conditional_game(spell_condition: bool, effect_condition: bool) -> soe_game::Game {
    let mut effect_record = effect(EFFECT, 0, 4, 24);
    if effect_condition {
        effect_record = effect_record.sub(T(b"CTDA"), condition(14, 26, 100.0));
    }
    let mut spell_record = spell(SPELL, EFFECT, 10, 20.0, 1);
    if spell_condition {
        spell_record = spell_record.sub(T(b"CTDA"), condition(14, 26, 100.0));
    }
    let game = fixture(vec![effect_record], vec![spell_record], false);
    for actor in [CASTER, TARGET, OTHER] {
        game.world_mut()
            .set_actor_value(id(actor), "stamina", 100.0)
            .unwrap();
    }
    game
}

#[test]
fn mag_70_each_condition_group_can_independently_end_the_live_application() {
    for (spell_condition, effect_condition) in [(true, false), (false, true)] {
        let mut game = conditional_game(spell_condition, effect_condition);
        cast(&mut game, SPELL, TARGET);
        game.tick(0.25);
        game.world_mut()
            .set_actor_value(id(TARGET), "stamina", 80.0)
            .unwrap();
        game.tick(0.75);

        assert_eq!(game.world().active_effects().count(), 0);
        assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 80.0);
    }
}

#[test]
fn mag_70_subsecond_tick_partition_preserves_the_second_check_boundary() {
    let mut whole = conditional_game(true, false);
    let mut split = conditional_game(true, false);
    cast(&mut whole, SPELL, TARGET);
    cast(&mut split, SPELL, TARGET);
    whole.tick(1.0);
    for _ in 0..4 {
        split.tick(0.25);
    }

    assert_eq!(whole.world().active_effects().count(), 1);
    assert_eq!(split.world().active_effects().count(), 1);
    assert_eq!(
        whole.world().active_effects().next().unwrap().elapsed,
        split.world().active_effects().next().unwrap().elapsed
    );
    assert_eq!(
        whole.world_mut().actor_value(id(TARGET), "health"),
        split.world_mut().actor_value(id(TARGET), "health")
    );
}

#[test]
fn mag_70_save_load_keeps_the_next_check_after_many_fractional_ticks() {
    let mut original = conditional_game(true, false);
    cast(&mut original, SPELL, TARGET);
    for _ in 0..7 {
        original.tick(0.1);
    }
    let bytes = original.save_state().unwrap();

    let mut restored = conditional_game(true, false);
    restored.load_state(&bytes).unwrap();
    restored
        .world_mut()
        .set_actor_value(id(TARGET), "stamina", 80.0)
        .unwrap();
    restored.tick(0.29);
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(0.01);
    assert_eq!(restored.world().active_effects().count(), 0);
}
