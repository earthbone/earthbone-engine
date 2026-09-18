// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Loads the five base masters from a real install through `soe_esm`,
//! builds a `Game`, loads its scripts through `Game::load_scripts_from_data`
//! (loose `Data/Scripts` if present, otherwise the plugin-named archives
//! such as `Skyrim - Misc.bsa`), then binds every `VMAD` script the load
//! order names. Gated on `SKYRIM_DATA` pointing at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-game --test real_files -- --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use soe_esm::{LoadOrder, Plugin};
use soe_game::Game;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

#[test]
fn the_installed_game_binds_its_vanilla_scripts() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the real-file test");
        return;
    };
    let mut order = LoadOrder::new();
    for name in MASTERS {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        order.add(name, plugin).unwrap();
    }
    let mut game = Game::new(order);

    let load_started = Instant::now();
    let scripts = game.load_scripts_from_data(&data);
    eprintln!(
        "load_scripts_from_data: {} archives, {} scripts found, {} loaded, {} failed, in {:.1}s",
        scripts.archives_loaded.len(),
        scripts.scripts_found,
        scripts.scripts_loaded,
        scripts.scripts_failed.len(),
        load_started.elapsed().as_secs_f32(),
    );
    for a in &scripts.archives_loaded {
        eprintln!("  archive: {}", a.display());
    }
    for (path, err) in scripts.scripts_failed.iter().take(20) {
        eprintln!("  failed: {path}: {err}");
    }

    let bind_started = Instant::now();
    let report = game.bind();
    eprintln!(
        "bind: {} forms with scripts, {} scripts attached, {} scripts missing, {} properties set, {} properties missing, {} properties read-only, {} quests started, in {:.1}s",
        report.forms_with_scripts,
        report.scripts_attached,
        report.scripts_missing.len(),
        report.properties_set,
        report.properties_missing.len(),
        report.properties_read_only.len(),
        report.quests_started,
        bind_started.elapsed().as_secs_f32(),
    );
    eprintln!("first 20 missing scripts:");
    for (script, form) in report.scripts_missing.iter().take(20) {
        eprintln!("  {script} on {form}");
    }
    eprintln!("first 20 missing properties:");
    for (script, prop) in report.properties_missing.iter().take(20) {
        eprintln!("  {script}.{prop}");
    }
<<<<<<< HEAD

    let start_started = Instant::now();
    let started = game.start();
    let mut calls: Vec<(String, u64)> = game
        .vm
        .natives
        .calls
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    calls.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: u64 = calls.iter().map(|c| c.1).sum();
    let errors = game
        .traces()
        .iter()
        .filter(|t| t.contains("no function") || t.contains("no global"))
        .count();
    eprintln!(
        "start: {} quests started, {} distinct natives reached, {} native calls, {} unresolved calls logged, {} log lines, in {:.1}s",
        started.quests_started,
        calls.len(),
        total,
        errors,
        game.traces().len(),
        start_started.elapsed().as_secs_f32(),
    );
    eprintln!("top 20 natives by call count:");
    for (name, n) in calls.iter().take(20) {
        eprintln!("  {n:6} {name}");
    }
    eprintln!("first 20 log lines:");
    for line in game.traces().iter().take(20) {
        eprintln!("  {line}");
    }
    assert!(
        !calls.is_empty(),
        "no native reached the game crate during start"
    );
=======
    eprintln!("first 20 read-only properties:");
    for (script, prop) in report.properties_read_only.iter().take(20) {
        eprintln!("  {script}.{prop}");
    }
>>>>>>> vm-props
    assert!(
        report.forms_with_scripts > 0,
        "no VMAD-carrying forms found"
    );
    assert!(
        scripts.scripts_loaded > 0,
        "no scripts loaded from the data view"
    );
}
