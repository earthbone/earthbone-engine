// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/magic_adversarial.rs"]
mod support;

use soe_game::magic_profile::MagicPhase;
use soe_game::world::{Pending, TranslationPath};
use soe_trace::System;
use support::*;

const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x310;

fn game_with(effect: soe_esm::Record, spell: soe_esm::Record) -> soe_game::Game {
    fixture(vec![effect], vec![spell], false)
}

fn active_effect_game() -> soe_game::Game {
    game_with(effect(EFFECT, 0, 4, 24), spell(SPELL, EFFECT, 3, 10.0, 1))
}

#[test]
fn prf_35_invalid_or_deleted_effects_do_not_create_live_applications() {
    let missing_spell = spell(0x311, 0x3ff, 3, 10.0, 0);
    let mut game = game_with(effect(EFFECT, 0, 4, 24), missing_spell);
    let result = game.shared_world().borrow_mut().cast_spell(
        id(0x311),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    assert!(result.is_err());
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
}

#[test]
fn prf_35_effect_expiry_matches_across_tick_partitions_and_save_reload() {
    let mut whole = active_effect_game();
    let mut split = active_effect_game();
    cast(&mut whole, SPELL, TARGET);
    cast(&mut split, SPELL, TARGET);
    whole.tick(3.25);
    for _ in 0..3 {
        split.tick(0.5);
    }
    let saved = split.save_state().unwrap();
    let mut restored = active_effect_game();
    restored.load_state(&saved).unwrap();
    restored.tick(1.75);

    for game in [&mut whole, &mut restored] {
        assert_eq!(game.world().active_effects().count(), 0);
        assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 70.0);
    }
}

#[test]
fn prf_35_archetype_index_rebuilds_after_load_and_removes_despelled_ids() {
    let mut game = game_with(effect(EFFECT, 45, 0, 24), spell(SPELL, EFFECT, 4, 10.0, 1));
    cast(&mut game, SPELL, TARGET);
    let selected = game.world().grab_actor_effects().next().unwrap().id;
    let saved = game.save_state().unwrap();
    game.shared_world()
        .borrow_mut()
        .dispel_effect(selected, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert_eq!(game.world().grab_actor_effects().count(), 0);

    let mut restored = game_with(effect(EFFECT, 45, 0, 24), spell(SPELL, EFFECT, 4, 10.0, 1));
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world().grab_actor_effects().next().unwrap().id,
        selected
    );
    restored
        .shared_world()
        .borrow_mut()
        .dispel_effect(selected, &mut |name, class| {
            restored.vm.create_object_of(name, class)
        });
    assert_eq!(restored.world().grab_actor_effects().count(), 0);
}

#[test]
fn prf_35_translation_index_preserves_rotation_gate_and_event_order() {
    let mut game = active_effect_game();
    game.world_mut().reference(id(TARGET)).unwrap().rotation = [0.0; 3];
    game.world_mut().start_translation(
        id(TARGET),
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 0.25],
        1.0,
        90.0,
        TranslationPath::Linear,
        &mut |_name, _class| soe_papyrus::ObjectId(1),
    );
    game.world_mut()
        .tick_translations(1.0, &mut |_name, _class| soe_papyrus::ObjectId(1));
    assert!(game
        .world()
        .reference_ref(id(TARGET))
        .unwrap()
        .translation
        .is_some());
    assert!(!game.world().pending.iter().any(|pending| matches!(
        pending,
        Pending::Event { name, .. } if name == "OnTranslationComplete"
    )));
    game.world_mut()
        .tick_translations(1.0, &mut |_name, _class| soe_papyrus::ObjectId(1));
    let world = game.world();
    let events: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, .. } if name.starts_with("OnTranslation") => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        events,
        ["OnTranslationAlmostComplete", "OnTranslationComplete"]
    );
    assert_eq!(
        game.world().reference_ref(id(TARGET)).unwrap().rotation,
        [0.0, 0.0, 0.25]
    );
}

#[test]
fn prf_35_reported_phases_sum_to_magic_and_paused_ticks_are_idle() {
    let mut game = active_effect_game();
    cast(&mut game, SPELL, TARGET);
    game.tick(0.1);
    let phases = MagicPhase::ALL
        .into_iter()
        .map(|phase| game.magic_frame_history().last_ns(phase))
        .sum::<u64>();
    assert_eq!(
        phases,
        game.frame_history().last().system_ns[System::Magic as usize]
    );
    assert!(
        game.magic_frame_history()
            .last_ns(MagicPhase::ActiveEffects)
            > 0
    );

    game.tick_menu_mode(1.0, true);
    for phase in MagicPhase::ALL {
        assert_eq!(game.magic_frame_history().last_ns(phase), 0);
    }
    assert!(game
        .magic_frame_history()
        .summary_text()
        .contains("magic.active_effects"));
}
