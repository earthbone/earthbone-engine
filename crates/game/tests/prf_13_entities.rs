// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-13 attribution through the real actor driver, VM and quest entry points.

use soe_esm::records::gameplay::{Comparison, Condition};
use soe_esm::{GlobalFormId, LoadOrder, Record, Tag};
use soe_game::{conditions, quest, Game};
use soe_papyrus::asm::{f, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode;
use soe_trace::{Operation, Profile};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_headless_environment_writes_a_session_on_game_drop() {
    let name = "prf_13_headless_environment_writes_a_session_on_game_drop";
    if std::env::var("SOE_PRF_13_CHILD").is_ok_and(|value| value == "1") {
        Profile::disable();
        Profile::clear();
        let mut game = fixture();
        assert!(Profile::enabled(), "Game did not honor SOE_PROFILE=1");
        assert!(game.set_stage(id(0x900), 10));
        drop(game);
        let directory = std::path::PathBuf::from(std::env::var_os("SOE_PROFILE_DIR").unwrap());
        assert!(std::fs::read_to_string(directory.join("soe-profile.json"))
            .unwrap()
            .contains("stage_changes"));
        assert!(std::fs::read_to_string(directory.join("soe-entities.json"))
            .unwrap()
            .contains("Profile.esm"));
        Profile::disable();
        return;
    }
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-13-headless-entities-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &crate::common::child_test_name(module_path!(), name),
            "--nocapture",
        ])
        .env("SOE_PRF_13_CHILD", "1")
        .env("SOE_PROFILE", "1")
        .env("SOE_PROFILE_DIR", &directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires release build for the fixed one-percent calibration"]
fn prf_13_disabled_capture_retains_prf_12_frame_budget() {
    const MAX_RATIO: f64 = 1.01;
    const PAIRS: usize = 15;
    Profile::disable();
    let baseline = std::env::var_os("SOE_PRF_13_BASELINE").expect("pre-change release probe path");
    let current = std::env::var_os("SOE_PRF_13_CURRENT").expect("current release probe path");
    let run = |path: &std::ffi::OsStr, suspend: bool| {
        let mut command = std::process::Command::new(path);
        command.env_remove("SOE_PROFILE");
        if suspend {
            command.arg("suspended");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = String::from_utf8(output.stdout).unwrap();
        let values = output
            .split_whitespace()
            .map(|n| n.parse::<u64>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 2);
        (values[0] as f64, values[1])
    };
    let mut ratios = Vec::with_capacity(PAIRS);
    for pair in 0..PAIRS {
        let (before, after) = if pair % 2 == 0 {
            (run(&baseline, true), run(&current, false))
        } else {
            let after = run(&current, false);
            (run(&baseline, true), after)
        };
        assert_eq!(before.1, after.1, "profiling hooks changed actor positions");
        ratios.push(after.0 / before.0);
    }
    ratios.sort_by(f64::total_cmp);
    let ratio = ratios[PAIRS / 2];
    eprintln!(
        "PRF-13 combined 1000-actor overhead: {:.3}%",
        (ratio - 1.0) * 100.0
    );
    assert!(
        ratio < MAX_RATIO,
        "combined disabled ratio {ratio:.6} exceeds {MAX_RATIO}"
    );
    crate::prf_12_frame::prf_12_accumulator_under_one_percent_of_thousand_actor_stress_tick();
}

fn isolated(test: &str) -> bool {
    if std::env::var("SOE_PRF_13_CHILD").is_ok_and(|value| value == "1") {
        return false;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &crate::common::child_test_name(module_path!(), test),
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SOE_PRF_13_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}

pub(super) fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

pub(super) fn fixture() -> Game {
    let (actors, _) = crate::prf_12_frame::actors(2).unwrap();
    let mut plugin = actors.world().order().files()[0].plugin.clone();
    let record = quest::build_record(0x900, "ProfileQuest", 0);
    let record = quest::with_alias(record, 0, "Actor", Some(0x1000));
    plugin.push_top(quest::with_stage(record, 10, 0, 0, "Profile stage"));
    for (object, tag) in [(0x901, *b"INFO"), (0x902, *b"PERK"), (0x903, *b"MGEF")] {
        plugin.push_top(Record::new(Tag(tag), object));
    }
    let mut order = LoadOrder::new();
    order.add("Profile.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [0x1000, 0x1001] {
        assert!(game.world_mut().drive_actor(id(actor)));
    }
    for (name, plugin) in [("First", "First.esp"), ("Second", "Second.esp")] {
        let script = ScriptBuilder::new(name, "")
            .function(FunctionBuilder::new("Empty", "None").global().build())
            .function(
                FunctionBuilder::new("Run", "None")
                    .global()
                    .local("result", "None")
                    .ins(
                        Opcode::CallStatic,
                        vec![s("Utility"), s("Wait"), ident("result"), f(0.125)],
                    )
                    .ins(Opcode::Return, vec![none()])
                    .build(),
            )
            .bytes();
        game.load_script(&script).unwrap();
        game.vm.set_script_plugin(name, plugin);
    }
    game
}

#[test]
fn prf_13_two_actors_two_scripts_one_quest_and_all_condition_callers() {
    if isolated("prf_13_two_actors_two_scripts_one_quest_and_all_condition_callers") {
        return;
    }
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    let mut game = fixture();
    Profile::clear();
    Profile::enable();
    assert!(game.start_quest(id(0x900)));
    assert!(game.set_stage(id(0x900), 10));
    assert!(game.world_mut().force_alias(id(0x900), 0, Some(id(0x1001))));
    game.vm.call_global("First", "Empty", &[]).unwrap();
    for name in ["First", "Second"] {
        game.vm.call_global(name, "Run", &[]).unwrap();
    }
    game.tick(0.0625);
    game.tick(0.0625);
    game.tick(0.0625);
    for owner in [0x900, 0x901, 0x902, 0x903, 0x803] {
        let context = conditions::Context {
            owner: id(owner),
            subject: id(0x1000),
            target: None,
            quest: Some(id(0x900)),
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        };
        let condition = Condition {
            flags: 0,
            comparison: Comparison::Number(1.0),
            function: 136,
            parameters: [0x1000, 0],
            run_on: 0,
            reference: 0,
            parameter_3: -1,
            strings: [None, None],
        };
        assert_eq!(
            conditions::evaluate(&mut game.world_mut(), &game.vm, context, &[condition]),
            Ok(true)
        );
    }
    Profile::disable();
    let rows = Profile::summary();
    for actor in [0x1000, 0x1001] {
        for phase in [
            "planning",
            "package_evaluation",
            "movement",
            "procedure_movement",
        ] {
            assert!(
                rows.iter().any(|(entity, stats)| entity.kind == "actor"
                    && entity.id == id(actor).to_string()
                    && entity.phase == phase
                    && stats.calls > 0
                    && stats.total_ns > 0),
                "missing actor {actor:#x} {phase}"
            );
        }
    }
    for (name, plugin) in [("First.Run", "First.esp"), ("Second.Run", "Second.esp")] {
        let execute = rows
            .iter()
            .find(|(entity, _)| entity.id == name && entity.phase == "execute")
            .unwrap();
        assert_eq!(execute.0.plugin, plugin);
        assert_eq!(execute.1.calls, 1);
        assert_eq!(execute.1.work.get(Operation::PapyrusInstructions), 2);
        assert_eq!(execute.1.work.get(Operation::NativeCalls), 1);
        let wait = rows
            .iter()
            .find(|(entity, _)| entity.id == name && entity.phase == "latent_wait")
            .unwrap();
        assert_eq!(wait.1.waits, 1);
        assert_eq!(wait.1.wait_ns, 125_000_000);
    }
    let empty = rows
        .iter()
        .find(|(entity, _)| entity.id == "First.Empty" && entity.phase == "execute")
        .unwrap();
    assert_eq!(empty.1.calls, 1);
    assert_eq!(empty.1.work.get(Operation::PapyrusInstructions), 0);
    assert!(empty.1.total_ns > 0);
    for phase in ["stage_changes", "alias_fills"] {
        assert!(rows.iter().any(|(entity, stats)| entity.kind == "quest"
            && entity.phase == phase
            && stats.calls > 0));
    }
    for phase in [
        "dialogue_topics",
        "perk_entry_points",
        "magic_effects",
        "packages",
        "quests",
    ] {
        assert!(
            rows.iter().any(|(entity, stats)| entity.kind == "condition"
                && entity.phase == phase
                && entity.plugin == "Profile.esm"
                && stats.work.get(Operation::Conditions) == 1),
            "missing {phase}"
        );
    }
    assert!(rows.iter().any(|(entity, stats)| entity.kind == "record"
        && entity.plugin == "Profile.esm"
        && stats.work.get(Operation::RecordLookups) > 0));
    assert_eq!(Profile::ranking(&rows, "actor").len(), 2);
    assert_eq!(Profile::ranking(&rows, "script").len(), 2);
    assert_eq!(Profile::ranking(&rows, "quest").len(), 1);
    if let Some(directory) = std::env::var_os("SOE_PRF_13_FIXTURE") {
        crate::ply_01_player_input::export_window_fixture(std::path::Path::new(&directory));
    }
}

#[test]
fn prf_13_installed_record_ownership_uses_defining_plugin() {
    if isolated("prf_13_installed_record_ownership_uses_defining_plugin") {
        return;
    }
    let _guard = LOCK.lock().unwrap();
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    Profile::clear();
    Profile::enable();
    for (file, loaded) in order.files().iter().enumerate() {
        let record = loaded
            .plugin
            .records
            .iter()
            .find(|record| order.resolve(file, record.form_id).file as usize == file)
            .unwrap();
        let form = order.resolve(file, record.form_id);
        assert!(order.winner(form).is_some());
    }
    Profile::disable();
    let rows = Profile::summary();
    for file in order.files() {
        assert!(rows.iter().any(|(entity, stats)| entity.kind == "record"
            && entity.plugin == file.name
            && stats.work.get(Operation::RecordLookups) > 0));
    }
}

#[test]
fn prf_13_dialogue_topics_and_perk_entries_keep_distinct_caller_ids() {
    if isolated("prf_13_dialogue_topics_and_perk_entries_keep_distinct_caller_ids") {
        return;
    }
    Profile::disable();
    let mut dialogue = crate::dialogue::prf_13_fixture();
    let mut perks = crate::perk_dispatch::prf_13_fixture();
    let actor = perks.player();
    let context = soe_game::perks::EntryPointContext {
        tabs: vec![actor],
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: None,
    };
    Profile::clear();
    Profile::enable();
    assert_eq!(dialogue.dialogue_choices().unwrap().available.len(), 2);
    for entry in [35, 36] {
        assert_eq!(
            soe_game::perks::entry_point_invocations(
                &mut perks.world_mut(),
                &perks.vm,
                actor,
                entry,
                &context
            )
            .unwrap()
            .len(),
            1
        );
    }
    Profile::disable();
    let rows = Profile::summary();
    for (topic, info) in [(0x900, 0xa00), (0x901, 0xa03)] {
        let expected = format!("{}/info/{}", id(topic), id(info));
        assert!(rows.iter().any(|(e, s)| e.kind == "condition"
            && e.id == expected
            && e.phase == "dialogue_topics"
            && e.plugin == "Dialogue.esm"
            && s.work.get(Operation::Conditions) > 0));
    }
    for (entry, section) in [(35, 0), (36, 1)] {
        let expected = format!("{}/entry/{entry}/section/{section}/tab/0", id(0x800));
        assert!(rows.iter().any(|(e, s)| e.kind == "condition"
            && e.id == expected
            && e.phase == "perk_entry_points"
            && e.plugin == "PerkDispatch.esm"
            && s.work.get(Operation::Conditions) == 1));
    }
}

#[test]
fn prf_13_overrides_keep_defining_plugin_attribution() {
    if isolated("prf_13_overrides_keep_defining_plugin_attribution") {
        return;
    }
    use soe_esm::{FileHeader, Plugin};
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"MGEF"), 0x800).zsub(Tag(*b"EDID"), "Base"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag(*b"MGEF"), 0x800).zsub(Tag(*b"EDID"), "Patched"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    Profile::clear();
    Profile::enable();
    assert_eq!(
        order.winner(id(0x800)).unwrap().editor_id().as_deref(),
        Some("Patched")
    );
    let winner = order.winner_ref(id(0x800)).unwrap();
    assert_eq!(winner.file, 1);
    assert_eq!(order.record(winner).editor_id().as_deref(), Some("Patched"));
    Profile::disable();
    let rows = Profile::summary();
    assert_eq!(rows.len(), 1);
    let (entity, stats) = rows.iter().next().unwrap();
    assert_eq!(entity.plugin, "Base.esm");
    assert_eq!(stats.calls, 2);
    assert_eq!(stats.work.get(Operation::RecordLookups), 2);
}

/// Runs `godot/prf_13_entities_test.gd` in a window against the built extension.
/// A key remap in the system overlay that skips this script now fails here.
/// Gated on SKYRIM_DATA and a display, like the other installed host checks.
#[test]
fn prf_13_windowed_host_overlay_script_passes_with_synthetic_audio_fixture() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-13-window-{}", std::process::id()));
    let fixture = scratch.join("fixture");
    crate::ply_01_player_input::export_window_fixture(&fixture);
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(&project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "prf_13_entities_test.gd",
        ])
        .env("SOE_PRF_13_FIXTURE", &fixture)
        .env("XDG_DATA_HOME", scratch.join("user"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::remove_dir_all(&scratch).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("prf_13_f4_input_presentation_and_session_export: PASS"),
        "{stdout}\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
