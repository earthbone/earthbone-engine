// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::gameplay::{Comparison, Condition};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{conditions, Game};
use soe_trace::{Operation, Profile};
use std::sync::Mutex;

static PROFILE_LOCK: Mutex<()> = Mutex::new(());

fn quest_context() -> conditions::Context {
    conditions::Context {
        owner: GlobalFormId {
            file: 0,
            object: 0x800,
        },
        subject: GlobalFormId {
            file: 0,
            object: 0x801,
        },
        target: None,
        quest: Some(GlobalFormId {
            file: 0,
            object: 0x800,
        }),
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    }
}

fn unsupported_condition() -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 0xFFFF,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn quest_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"QUST"), 0x800));
    let mut order = LoadOrder::new();
    order.add("QuestOwner.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prf_13_rejected_quest_predicates_still_charge_the_quest_and_plugin() {
    let _guard = PROFILE_LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    let game = quest_game();
    let context = quest_context();
    let rejected = unsupported_condition();

    Profile::enable();
    for _ in 0..2 {
        assert!(conditions::evaluate(
            &mut game.world_mut(),
            &game.vm,
            context,
            std::slice::from_ref(&rejected)
        )
        .is_err());
    }
    Profile::disable();

    let rows = Profile::summary();
    let owner = context.owner.to_string();
    let condition = rows
        .iter()
        .find(|(entity, _)| entity.kind == "condition" && entity.id == owner)
        .unwrap();
    assert_eq!(condition.0.plugin, "QuestOwner.esm");
    assert_eq!(condition.1.work.get(Operation::Conditions), 2);
    let quest = rows
        .iter()
        .find(|(entity, _)| entity.kind == "quest" && entity.id == owner)
        .unwrap();
    assert_eq!(quest.0.plugin, "QuestOwner.esm");
    assert_eq!(quest.1.calls, 2);
    if let Some(path) = std::env::var_os("SOE_PRF_13_FIXTURE") {
        crate::ply_01_player_input::export_window_fixture(std::path::Path::new(&path));
    }
}

#[test]
fn prf_13_synthetic_actor_script_and_quest_identities_are_distinct() {
    let _guard = PROFILE_LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    let mut game = crate::prf_13_entities::fixture();
    Profile::enable();
    let quest = crate::prf_13_entities::id(0x900);
    assert!(game.start_quest(quest));
    assert!(game.set_stage(quest, 10));
    for script in ["First", "Second"] {
        game.vm.call_global(script, "Run", &[]).unwrap();
    }
    for _ in 0..3 {
        game.tick(0.0625);
    }
    Profile::disable();

    let rows = Profile::summary();
    for actor in [0x1000, 0x1001] {
        assert!(rows.iter().any(|(entity, stats)| entity.kind == "actor"
            && entity.id == crate::prf_13_entities::id(actor).to_string()
            && entity.phase == "planning"
            && stats.calls > 0));
    }
    for (script, plugin) in [("First.Run", "First.esp"), ("Second.Run", "Second.esp")] {
        let (entity, stats) = rows
            .iter()
            .find(|(entity, _)| entity.id == script && entity.phase == "execute")
            .unwrap();
        assert_eq!(entity.plugin, plugin);
        assert_eq!(stats.calls, 1);
        assert!(stats.work.get(Operation::PapyrusInstructions) > 0);
    }
    assert!(rows.iter().any(|(entity, stats)| entity.kind == "quest"
        && entity.id == quest.to_string()
        && entity.phase == "stage_changes"
        && stats.calls > 0));
    assert_eq!(Profile::ranking(&rows, "actor").len(), 2);
    assert_eq!(Profile::ranking(&rows, "script").len(), 2);
    assert_eq!(Profile::ranking(&rows, "quest").len(), 1);
}

#[test]
fn prf_13_headless_environment_starts_and_exports_entity_reports() {
    let name = "prf_13_headless_environment_starts_and_exports_entity_reports";
    if std::env::var("SOE_PRF_13_CHILD").is_ok_and(|value| value == "1") {
        Profile::disable();
        Profile::clear();
        let game = quest_game();
        assert!(Profile::enabled());
        assert!(conditions::evaluate(
            &mut game.world_mut(),
            &game.vm,
            quest_context(),
            &[unsupported_condition()]
        )
        .is_err());
        drop(game);
        let directory = std::path::PathBuf::from(std::env::var_os("SOE_PROFILE_DIR").unwrap());
        let trace = std::fs::read_to_string(directory.join("soe-profile.json")).unwrap();
        let summary = std::fs::read_to_string(directory.join("soe-entities.json")).unwrap();
        assert!(trace.contains("condition"));
        assert!(trace.contains("QuestOwner.esm"));
        assert!(summary.contains("QuestOwner.esm"));
        Profile::disable();
        return;
    }

    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-13-headless-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &format!("prf_13_adversarial::{name}"),
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
