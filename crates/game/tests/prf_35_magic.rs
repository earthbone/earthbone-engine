// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-35 checks record-derived tick work, live conditions and existing clocks.

use crate::support::magic_adversarial::{effect, id, put, spell, BASE, CASTER, TARGET};
use soe_esm::{FileHeader, LoadOrder, Plugin, Tag};
use soe_game::{npc, Game};
use soe_trace::{Operation, WorkFrame};

const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;

fn fixture(flags: u32, constant: bool, conditional: bool, actors: usize) -> Game {
    fixture_kind(flags, constant, conditional, actors, 0)
}

fn fixture_kind(
    flags: u32,
    constant: bool,
    conditional: bool,
    actors: usize,
    archetype: u32,
) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        BASE,
        "Base",
        "Base",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    let mut mgef = effect(EFFECT, archetype, flags, 24);
    put(&mut mgef.subrecords[0].data, 80, u32::from(!constant));
    plugin.push_top(mgef);
    let mut definition = spell(SPELL, EFFECT, 3, 10.0, u32::from(!constant));
    if conditional {
        let mut condition = vec![0; 32];
        put(&mut condition, 4, 100.0_f32.to_bits());
        condition[8..10].copy_from_slice(&14_u16.to_le_bytes());
        put(&mut condition, 12, 26);
        definition = definition.sub(Tag(*b"CTDA"), condition);
    }
    plugin.push_top(definition);
    for actor in [CASTER, TARGET]
        .into_iter()
        .chain((0..actors).map(|n| 0x1000 + n as u32))
    {
        plugin.push_top(crate::support::magic_adversarial::actor(actor, [0.0; 3]));
    }
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().debug_ai_enabled = false;
    for actor in [CASTER, TARGET]
        .into_iter()
        .chain((0..actors).map(|n| 0x1000 + n as u32))
    {
        for value in ["health", "stamina"] {
            game.world_mut()
                .set_actor_value(id(actor), value, 100.0)
                .unwrap();
        }
    }
    game
}

fn cast(game: &mut Game, target: u32) {
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(target)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn prf_35_unchanged_modifiers_advance_clocks_without_record_lookups() {
    let mut game = fixture(2, true, false, 1000);
    for actor in 0x1000..0x1008 {
        cast(&mut game, actor);
    }
    game.tick(0.1);
    let work = WorkFrame::begin();
    let errors = game
        .shared_world()
        .borrow_mut()
        .tick_magic(0.25, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert!(errors.is_empty());
    assert_eq!(work.counts().get(Operation::RecordLookups), 0);
    for active in game.world().active_effects() {
        assert!((active.elapsed - 0.35).abs() < 1e-9);
    }
    assert_eq!(game.world_mut().actor_value(id(0x1000), "health"), 110.0);
}

#[test]
fn prf_35_final_partial_damage_and_expiry_survive_cache_reuse() {
    let mut game = fixture(4, false, false, 0);
    cast(&mut game, TARGET);
    game.tick(0.25);
    game.tick(4.0);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 70.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn prf_35_live_conditions_recheck_on_second_boundary_after_reload() {
    let mut game = fixture(2, false, true, 0);
    cast(&mut game, TARGET);
    game.tick(0.5);
    let saved = game.save_state().unwrap();
    let mut restored = fixture(2, false, true, 0);
    restored.load_state(&saved).unwrap();
    restored
        .world_mut()
        .set_actor_value(id(TARGET), "stamina", 80.0)
        .unwrap();
    restored.tick(0.49);
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(0.01);
    assert_eq!(restored.world().active_effects().count(), 0);
    assert_eq!(
        restored.world_mut().actor_value(id(TARGET), "health"),
        100.0
    );
}

#[test]
fn prf_35_death_dispels_cached_modifier_and_menu_pause_preserves_age() {
    let mut game = fixture(2, true, false, 0);
    cast(&mut game, TARGET);
    game.tick(0.25);
    game.tick_menu_mode(5.0, true);
    assert_eq!(game.world().active_effects().next().unwrap().elapsed, 0.25);
    game.world_mut().actor(id(TARGET)).unwrap().dead = true;
    game.tick(0.1);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
}

#[test]
fn prf_35_no_death_dispel_remains_active_when_target_dies() {
    let mut game = fixture(2 | (1 << 28), true, false, 0);
    cast(&mut game, TARGET);
    game.tick(0.25);
    game.world_mut().actor(id(TARGET)).unwrap().dead = true;
    game.tick(0.1);
    assert_eq!(game.world().active_effects().count(), 1);
}

#[test]
fn prf_35_host_effect_selection_rebuilds_on_load_and_removes_dispelled_ids() {
    let mut game = fixture_kind(0, false, false, 0, 45);
    cast(&mut game, TARGET);
    let selected = game.world().grab_actor_effects().next().unwrap().id;
    let saved = game.save_state().unwrap();
    game.shared_world()
        .borrow_mut()
        .dispel_effect(selected, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert_eq!(game.world().grab_actor_effects().count(), 0);
    let mut restored = fixture_kind(0, false, false, 0, 45);
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world().grab_actor_effects().next().unwrap().id,
        selected
    );
}

#[test]
fn prf_35_phase_report_covers_tick_work_and_paused_idle_ticks() {
    use soe_game::magic_profile::MagicPhase;
    let mut game = fixture(2, false, false, 0);
    cast(&mut game, TARGET);
    game.tick(0.1);
    let history = game.magic_frame_history();
    assert!(history.last_ns(MagicPhase::ActiveEffects) > 0);
    for phase in MagicPhase::ALL {
        assert!(history.summary_text().contains(phase.name()));
    }
    game.tick_menu_mode(1.0, true);
    for phase in MagicPhase::ALL {
        assert_eq!(game.magic_frame_history().last_ns(phase), 0);
    }
}

#[test]
fn prf_35_translation_index_discovers_direct_mutation_and_preserves_terminal_events() {
    use soe_game::world::{Pending, RefTranslation, TranslationPath};
    let mut game = fixture(2, false, false, 1000);
    game.tick(0.1);
    game.world_mut().reference(id(TARGET)).unwrap().translation = Some(RefTranslation {
        start_position: [0.0; 3],
        target_position: [10.0, 0.0, 0.0],
        start_rotation: [0.0; 3],
        target_rotation: [0.0; 3],
        speed: 10.0,
        max_rotation_speed: 0.0,
        path: TranslationPath::Linear,
        elapsed: 0.0,
        duration: 1.0,
        almost_complete_fired: false,
    });
    game.shared_world()
        .borrow_mut()
        .tick_translations(1.0, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    let world = game.world();
    assert_eq!(
        world.reference_ref(id(TARGET)).unwrap().position,
        [10.0, 0.0, 0.0]
    );
    assert_eq!(world.reference_ref(id(0x1000)).unwrap().position, [0.0; 3]);
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
}

#[test]
fn prf_35_translation_index_rebuilds_on_load_and_survives_a_rejected_load() {
    use soe_game::world::TranslationPath;
    let mut game = fixture(2, false, false, 0);
    game.shared_world().borrow_mut().start_translation(
        id(TARGET),
        [10.0, 0.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    let saved = game.save_state().unwrap();
    let mut restored = fixture(2, false, false, 0);
    restored.load_state(&saved).unwrap();
    restored.tick(0.25);
    assert_eq!(
        restored.world().reference_ref(id(TARGET)).unwrap().position,
        [2.5, 0.0, 0.0]
    );
    let mut corrupt = restored.world().save_state();
    corrupt.truncate(corrupt.len() / 2);
    assert!(restored.world_mut().load_state(&corrupt).is_err());
    restored.tick(0.25);
    assert_eq!(
        restored.world().reference_ref(id(TARGET)).unwrap().position,
        [5.0, 0.0, 0.0]
    );
}

#[test]
fn prf_35_installed_windowed_magic_budget() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        eprintln!("SKIP prf_35_installed_windowed_magic_budget: SKYRIM_DATA unset");
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN must name Godot 4.4");
    let report = root.join("target/prf-35-after.json");
    let log = root.join("target/prf-35-after.log");
    let output = std::process::Command::new(godot)
        .current_dir(root)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "prf_35_magic_test.gd",
        ])
        .env_remove("SOE_PRF_35_BASELINE")
        .env("SOE_PRF_35_REPORT", &report)
        .output()
        .unwrap();
    let mut captured = output.stdout;
    captured.extend(output.stderr);
    std::fs::write(&log, captured).unwrap();
    assert!(
        output.status.success(),
        "Godot exited {}. See {}",
        output.status,
        log.display()
    );
    assert!(std::fs::metadata(report).unwrap().len() > 0);
}
