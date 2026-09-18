// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The determinism detector on the two synthetic scenarios and on the
//! installed game: one seed, two runs, every tick's hash compared, and a
//! failure names the first tick and subsystem that differ. The scenario
//! builders come from the sibling test files, included as modules, so their
//! own tests run here a second time.
//!
//! Two-process runs spawn this test binary again with `SOE_HASH_DUMP` set,
//! the child writes its hash log to that file and the parent compares.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use soe_esm::{LoadOrder, Plugin};
use soe_game::determinism::{compare, dump_if_requested, from_text, TickHash};
use soe_game::Game;

#[allow(dead_code)]
#[path = "quest_flow.rs"]
mod quest_flow;

#[allow(dead_code)]
#[path = "packages.rs"]
mod packages;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

/// The quest flow: start, pull the lever, walk through the door, open the
/// chest, then three seconds on the clock so the timer fires. A zero-length
/// tick after each step records a hash without moving time.
fn run_quest_flow() -> Vec<TickHash> {
    let mut game = quest_flow::game();
    game.record_hashes();
    game.start();
    game.tick(0.0);
    let player = game.player();
    let lever = game.form("LeverRef").unwrap();
    let door_a = game.form("DoorA").unwrap();
    let chest = game.form("ChestRef").unwrap();
    assert!(game.activate(lever, player));
    game.tick(0.0);
    assert!(game.activate(door_a, player));
    game.tick(0.0);
    assert!(game.activate(chest, player));
    for _ in 0..30 {
        game.tick(0.1);
    }
    game.hash_log().to_vec()
}

/// The travel package: the guard walks the navmesh from cell A into cell B
/// and arrives, 3.6 seconds at ten ticks a second.
fn run_travel() -> Vec<TickHash> {
    let mut game = packages::game_at_hour(12.0);
    game.record_hashes();
    let quest = game.form("GuardQuest").unwrap();
    game.set_stage(quest, 10);
    packages::tick(&mut game, 3.6);
    game.hash_log().to_vec()
}

fn assert_same(name: &str, a: &[TickHash], b: &[TickHash]) {
    if let Err(d) = compare(a, b) {
        panic!("{name}: {d}");
    }
    assert!(!a.is_empty(), "{name}: nothing was recorded");
}

#[test]
fn the_quest_flow_hashes_the_same_twice_in_one_process() {
    let a = run_quest_flow();
    let b = run_quest_flow();
    assert_same("quest flow", &a, &b);
}

#[test]
fn the_travel_package_hashes_the_same_twice_in_one_process() {
    let a = run_travel();
    let b = run_travel();
    assert_same("travel", &a, &b);
}

/// Run one named test of this binary in a child process with
/// `SOE_HASH_DUMP` pointing at a fresh file, and return the log it wrote.
fn child_log(test_name: &str) -> Vec<TickHash> {
    let exe = std::env::current_exe().unwrap();
    let path =
        std::env::temp_dir().join(format!("soe-hash-{test_name}-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let status = Command::new(exe)
        .arg(test_name)
        .arg("--exact")
        .env("SOE_HASH_DUMP", &path)
        .env_remove("SKYRIM_DATA")
        .status()
        .unwrap();
    assert!(
        status.success(),
        "child run of {test_name} failed: {status}"
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    from_text(&text)
}

/// The child half of the two-process quest flow test. Does nothing unless
/// `SOE_HASH_DUMP` is set, so a plain `cargo test` runs it as a no-op.
#[test]
fn quest_flow_hash_child() {
    if std::env::var_os("SOE_HASH_DUMP").is_none() {
        return;
    }
    dump_if_requested(&run_quest_flow()).unwrap();
}

#[test]
fn travel_hash_child() {
    if std::env::var_os("SOE_HASH_DUMP").is_none() {
        return;
    }
    dump_if_requested(&run_travel()).unwrap();
}

#[test]
fn the_quest_flow_hashes_the_same_in_another_process() {
    let here = run_quest_flow();
    let there = child_log("quest_flow_hash_child");
    assert_same("quest flow across processes", &here, &there);
}

#[test]
fn the_travel_package_hashes_the_same_in_another_process() {
    let here = run_travel();
    let there = child_log("travel_hash_child");
    assert_same("travel across processes", &here, &there);
}

fn real_game(data: &Path) -> Game {
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order.add(name, plugin).unwrap();
    }
    let mut game = Game::new(order);
    let scripts = game.load_scripts_from_data(data);
    assert!(scripts.scripts_loaded > 0, "{scripts:?}");
    game
}

/// The first real start, run twice, 600 ticks at 60 a second each.
fn run_real_start(data: &Path) -> Vec<TickHash> {
    let started = Instant::now();
    let mut game = real_game(data);
    game.record_hashes();
    let report = game.start();
    game.tick(0.0);
    for _ in 0..600 {
        game.tick(1.0 / 60.0);
    }
    eprintln!(
        "real start: {} quests started, {} ticks hashed, in {:.1}s",
        report.quests_started,
        game.hash_log().len(),
        started.elapsed().as_secs_f32()
    );
    game.hash_log().to_vec()
}

/// The first divergence the detector found (2026-09-18): `Game::start`
/// started quests in `LoadOrder::winners_of` order, which walked a
/// `HashMap`, so two runs started the 423 quests in different orders and
/// every subsystem differed from tick 0. `winners_of` now yields id order,
/// see `docs/validation.md`, "Determinism", and this run is the acceptance
/// check for that.
#[test]
fn the_installed_game_hashes_the_same_over_six_hundred_ticks() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let a = run_real_start(&data);
    let b = run_real_start(&data);
    assert_same("installed game", &a, &b);
}

/// The same run with the quest start order fixed from the outside: bind,
/// then start the Start Game Enabled quests in form id order, then 600
/// ticks. This isolates the start-order cause from anything after it.
fn run_real_start_sorted(data: &Path) -> Vec<TickHash> {
    let started = Instant::now();
    let mut game = real_game(data);
    game.record_hashes();
    game.bind();
    let mut quests: Vec<soe_esm::GlobalFormId> = game
        .world()
        .order()
        .winners_of(soe_esm::Tag::new(b"QUST"))
        .map(|(id, _)| id)
        .collect();
    quests.sort_unstable();
    let mut started_count = 0;
    for id in quests {
        let starts = game
            .world_mut()
            .quest_state(id)
            .is_some_and(|q| q.def.starts_enabled());
        if starts && game.start_quest(id) {
            started_count += 1;
        }
    }
    game.run_until_idle(64);
    game.tick(0.0);
    for _ in 0..600 {
        game.tick(1.0 / 60.0);
    }
    eprintln!(
        "real start, sorted: {started_count} quests started, {} ticks hashed, in {:.1}s",
        game.hash_log().len(),
        started.elapsed().as_secs_f32()
    );
    game.hash_log().to_vec()
}

#[test]
fn the_installed_game_hashes_the_same_when_quests_start_in_form_order() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let a = run_real_start_sorted(&data);
    let b = run_real_start_sorted(&data);
    assert_same("installed game, sorted starts", &a, &b);
}
