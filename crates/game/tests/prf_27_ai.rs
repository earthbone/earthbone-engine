// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-27 measurements through the live actor driver and form-backed world.

use soe_game::ai::metrics::{ProcessLevel, Stimulus};

#[test]
fn prf_27_permanent_phases_cover_packages_schedules_sandbox_and_steering() {
    let (mut game, ids) = crate::prf_12_frame::actors(12).unwrap();
    let cell = soe_esm::GlobalFormId {
        file: 0,
        object: 0x800,
    };
    for actor in &ids {
        game.world_mut().reference(*actor).unwrap().cell = Some(cell);
    }
    let mut observed = std::collections::BTreeSet::new();
    for _ in 0..6 {
        game.tick(1.0 / 60.0);
        observed.extend(
            game.world()
                .ai
                .planning_phases
                .iter()
                .filter(|(_, duration)| !duration.is_zero())
                .map(|(phase, _)| *phase),
        );
    }
    let world = game.world();
    for phase in [
        "package_selection",
        "schedule_evaluation",
        "combat_target",
        "combat_acquisition",
        "greeting_evaluation",
        "force_greet_evaluation",
        "sandbox_markers",
        "movement",
        "steering",
        "procedure.Travel",
        "procedure.Sandbox",
    ] {
        assert!(observed.contains(phase), "{phase}");
    }
    assert_eq!(
        world.ai.metrics.levels[ProcessLevel::High as usize].actors,
        ids.len() as u64
    );
    assert!((world.ai.metrics.levels[0].mean_interval_seconds - 1.0 / 60.0).abs() < 1e-10);
    assert!(!world.ai.path_metrics().is_empty());
    assert_eq!(world.ai.metrics.actors.len(), ids.len());
    drop(world);
    game.tick_menu_mode(1.0, true);
    let before = game.world().ai.metrics.actors[&ids[0]].updates;
    game.tick(1.0 / 60.0);
    assert_eq!(game.world().ai.metrics.actors[&ids[0]].updates, before + 1);
}

fn reaction_game() -> soe_game::Game {
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x14u32, 0x801] {
        plugin
            .push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    }
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x802).sub(Tag(*b"DATA"), 0x40u32.to_le_bytes()));
    for (index, (name, value)) in [
        ("fSneakBaseValue", 0.0f32),
        ("fSneakMaxDistance", 100.0),
        ("fSneakDistanceAttenuationExponent", 1.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 0.0),
        ("fSneakEquippedWeightMult", 1.0),
        ("fSneakRunningMult", 1.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.0),
        ("fSneakLightRunMult", 0.0),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 1.0),
        ("fSneakStealthBoyMult", 1.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 100.0),
        ("fSneakAlertMod", 0.0),
        ("fSneakCombatMod", 0.0),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 5.0),
        ("fCombatStealthPointRegenLostWaitTime", 5.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x900 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    for (index, name) in [
        "iCombatStealthPointDetectionThreshold",
        "iCombatStealthPointSneakDetectionThreshold",
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0xA00 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), 50i32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Prf27.esm", plugin).unwrap();
    soe_game::Game::new(order)
}

#[test]
fn prf_27_live_driver_flags_stalled_paths_and_clears_after_progress() {
    let (mut game, ids) = crate::prf_12_frame::actors(2).unwrap();
    game.tick(0.1);
    let actor = ids[0];
    let position = game.world().reference_ref(actor).unwrap().position;
    assert!(!game.world().actor_ref(actor).unwrap().ai.path.is_empty());
    for _ in 0..60 {
        game.world_mut().reference(actor).unwrap().position = position;
        game.tick(0.1);
    }
    assert!(game.world().ai.metrics.actors[&actor].stuck);
    game.tick(0.1);
    assert!(!game.world().ai.metrics.actors[&actor].stuck);
}

#[test]
fn prf_27_dialogue_latency_records_success_and_rejects_busy_conversations() {
    let mut game = reaction_game();
    let player = game.player();
    let speaker = soe_esm::GlobalFormId {
        file: 0,
        object: 0x801,
    };
    game.begin_dialogue(speaker, player).unwrap();
    let kind = Stimulus::PlayerDialogue as usize;
    assert_eq!(
        game.world().ai.metrics.actors[&speaker].reactions[kind].responses,
        1
    );
    assert!(game.begin_dialogue(speaker, player).is_err());
    assert_eq!(
        game.world().ai.metrics.actors[&speaker].reactions[kind].responses,
        1
    );
}

#[test]
fn prf_27_player_crime_latency_finishes_at_actual_assault_alarm() {
    let mut game = reaction_game();
    let player = game.player();
    let witness = soe_esm::GlobalFormId {
        file: 0,
        object: 0x801,
    };
    let faction = soe_esm::GlobalFormId {
        file: 0,
        object: 0x802,
    };
    let mut world = game.world_mut();
    world.actor(player).unwrap().crime_faction = Some(faction);
    world.actor(witness).unwrap().factions.insert(faction, 0);
    world.reference(player).unwrap();
    world.reference(witness).unwrap();
    world
        .record_witnessed_crime(player, witness, soe_game::crime::CrimeType::Murder)
        .unwrap();
    let kind = Stimulus::PlayerCrime as usize;
    assert!(world.ai.metrics.actors[&witness].reactions[kind]
        .pending
        .is_some());
    assert_eq!(
        world.ai.metrics.actors[&witness].reactions[kind].responses,
        0
    );
    world.send_assault_alarm(faction).unwrap();
    assert_eq!(
        world.ai.metrics.actors[&witness].reactions[kind].responses,
        1
    );
    assert_eq!(
        world.ai.metrics.actors[&witness].reactions[kind].response,
        Some(soe_game::ai::metrics::Response::Alarm)
    );
}

#[test]
fn prf_27_installed_bannered_mare_reports_live_procedures_and_paths() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let order = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let mut game = soe_game::Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .unwrap();
    game.load_cell(cell);
    let references = game.world_mut().cell_references(cell);
    for actor in references {
        if game
            .world()
            .record(actor)
            .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
        {
            game.world_mut().drive_actor(actor);
        }
    }
    for _ in 0..120 {
        game.tick(1.0 / 60.0);
    }
    let world = game.world();
    assert!(world.ai.metrics.levels[0].actors >= 5);
    assert!(world
        .ai
        .planning_phases
        .keys()
        .any(|phase| phase.starts_with("procedure.")));
    assert!(world
        .ai
        .path_metrics()
        .values()
        .any(|metric| metric.completed > 0));
}

#[test]
#[ignore = "requires release build and an idle machine"]
fn prf_27_retains_prf_12_accumulator_budget() {
    crate::prf_12_frame::prf_12_accumulator_under_one_percent_of_thousand_actor_stress_tick();
}

#[test]
fn prf_27_detected_player_latency_finishes_at_combat_acquisition() {
    use soe_game::detection::{Motion, Observation};
    let mut game = reaction_game();
    let player = game.player();
    let observer = soe_esm::GlobalFormId {
        file: 0,
        object: 0x801,
    };
    let observation = Observation {
        observer,
        target: player,
        distance: 0.0,
        line_of_sight: true,
        light_level: 1000.0,
        motion: Motion::Still,
        action_sound: 1000.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    };
    let kind = Stimulus::DetectedPlayer as usize;
    {
        let mut world = game.world_mut();
        world.set_actor_value(observer, "aggression", 2.0).unwrap();
        world.evaluate_detection(&[observation], 1.0).unwrap();
        assert!(world.ai.metrics.actors[&observer].reactions[kind]
            .pending
            .is_some());
        let before = world.ai.metrics.actors[&observer].reactions[kind].pending;
        world.evaluate_detection(&[observation], 1.0).unwrap();
        assert_eq!(
            world.ai.metrics.actors[&observer].reactions[kind].pending,
            before
        );
    }
    game.tick(0.1);
    let world = game.world();
    assert!(world.actor_ref(observer).unwrap().in_combat);
    assert_eq!(
        world.ai.metrics.actors[&observer].reactions[kind].response,
        Some(soe_game::ai::metrics::Response::Combat)
    );
    assert_eq!(
        world.ai.metrics.actors[&observer].reactions[kind].responses,
        1
    );
}

#[test]
#[ignore = "requires pre-change and current release probes on an idle machine"]
fn prf_27_combined_measurements_stay_under_one_percent() {
    const MAX_RATIO: f64 = 1.01;
    const PAIRS: usize = 15;
    let baseline = std::env::var_os("SOE_PRF_27_BASELINE").expect("baseline probe path");
    let current = std::env::var_os("SOE_PRF_27_CURRENT").expect("current probe path");
    let run = |path: &std::ffi::OsStr, suspended: bool| {
        let mut command = std::process::Command::new(path);
        command.env_remove("SOE_PROFILE");
        if suspended {
            command.arg("suspended");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let values: Vec<u64> = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 2);
        (values[0], values[1])
    };
    let mut ratios = Vec::with_capacity(PAIRS);
    for pair in 0..PAIRS {
        let (before, after) = if pair % 2 == 0 {
            (run(&baseline, true), run(&current, false))
        } else {
            let after = run(&current, false);
            (run(&baseline, true), after)
        };
        assert_eq!(before.1, after.1, "measurements changed actor positions");
        ratios.push(after.0 as f64 / before.0 as f64);
    }
    ratios.sort_by(f64::total_cmp);
    let ratio = ratios[PAIRS / 2];
    eprintln!(
        "PRF-27 combined measurements: median on/off {ratio:.6}, overhead {:.3}%",
        (ratio - 1.0) * 100.0
    );
    assert!(
        ratio < MAX_RATIO,
        "measurement ratio {ratio:.6} exceeds {MAX_RATIO}"
    );
}
