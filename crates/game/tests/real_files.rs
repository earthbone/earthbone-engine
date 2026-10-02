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

use crate::common::installed;

use std::path::PathBuf;
use std::time::Instant;

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
    let order = installed::load_order(&MASTERS).unwrap();
    let mut game = Game::from_shared_order(order);

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

    game.vm.start_coverage();
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
    eprintln!("first 20 read-only properties:");
    for (script, prop) in report.properties_read_only.iter().take(20) {
        eprintln!("  {script}.{prop}");
    }

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
        .filter(|t| {
            t.contains("no function") || t.contains("no global") || t.contains("no implementation")
        })
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
    let mut unresolved: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for line in game.traces() {
        // `[Class <edid>] has no function F() (script line n)`,
        // `no global function Class.F() (...)`,
        // `native Class.F() has no implementation (...)`.
        let function = |rest: &str| rest.split("()").next().unwrap_or(rest).to_string();
        let key = if let Some(rest) = line.split(" has no function ").nth(1) {
            let object = line.split(" has no function ").next().unwrap_or("");
            let class = object
                .rsplit('[')
                .next()
                .and_then(|o| o.split(" <").next())
                .unwrap_or(object);
            format!("{class}.{}", function(rest))
        } else if let Some(rest) = line.split("no global function ").nth(1) {
            function(rest)
        } else if let Some(rest) = line.split("native ").nth(1) {
            if !rest.contains("has no implementation") {
                continue;
            }
            function(rest)
        } else {
            continue;
        };
        *unresolved.entry(key).or_default() += 1;
    }
    let mut unresolved: Vec<(String, u64)> = unresolved.into_iter().collect();
    unresolved.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    eprintln!(
        "unresolved calls by function ({} distinct):",
        unresolved.len()
    );
    for (name, n) in &unresolved {
        eprintln!("  {n:6} {name}");
    }
    for line in game.traces().iter().filter(|t| {
        t.contains("no function") || t.contains("no global") || t.contains("no implementation")
    }) {
        eprintln!("  {line}");
    }

    // What the loaded scripts declare as native and the registry lacks,
    // with the static call-site count per function name so the next round
    // is ordered by demand. The count is by name alone, so a name shared
    // between classes adds up.
    let by_script = game.vm.call_sites_by_script();
    let mut sites: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
    for ((script, name), n) in &by_script {
        let e = sites.entry(name.clone()).or_default();
        e.0 += n;
        // Dawnguard, Dragonborn and HearthFires script prefixes.
        if script.starts_with("dlc1") || script.starts_with("dlc2") || script.starts_with("byoh") {
            e.1 += n;
        }
    }
    let mut missing: Vec<(u64, u64, String, String, bool)> = game
        .vm
        .unregistered_natives()
        .into_iter()
        .map(|(class, name, global)| {
            let (n, dlc) = sites
                .get(&name.to_ascii_lowercase())
                .copied()
                .unwrap_or((0, 0));
            (n, dlc, class, name, global)
        })
        .collect();
    missing.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
    let all_declared = game.vm.declared_natives();
    let declared = all_declared.len();
    // A script named for a DLC declaring a native of its own, which is the
    // tightest reading of a DLC native. The DLC additions to the base
    // classes sit in the base class scripts instead and show up above.
    let dlc_declared: Vec<&(String, String, bool)> = all_declared
        .iter()
        .filter(|(class, _, _)| {
            let c = class.to_ascii_lowercase();
            c.starts_with("dlc1") || c.starts_with("dlc2") || c.starts_with("byoh")
        })
        .collect();
    eprintln!(
        "natives declared by a DLC-named script: {}",
        dlc_declared.len()
    );
    for (class, name, global) in &dlc_declared {
        let kind = if *global { "global" } else { "method" };
        let covered = if game.vm.natives.has_method(class, name) {
            "registered"
        } else {
            "unregistered"
        };
        eprintln!("  {class}.{name} ({kind}, {covered})");
    }
    eprintln!(
        "declared natives: {declared}, unregistered: {} ({} with a call site, {} called from a DLC script)",
        missing.len(),
        missing.iter().filter(|m| m.0 > 0).count(),
        missing.iter().filter(|m| m.1 > 0).count()
    );
    eprintln!("unregistered natives, all sites then DLC sites:");
    for (n, dlc, class, name, global) in &missing {
        let kind = if *global { "global" } else { "method" };
        eprintln!("  {n:6} {dlc:5} {class}.{name} ({kind})");
    }
    assert!(
        unresolved.is_empty(),
        "the first start left calls unresolved: {unresolved:?}"
    );
    assert!(
        !calls.is_empty(),
        "no native reached the game crate during start"
    );
    assert!(
        report.forms_with_scripts > 0,
        "no VMAD-carrying forms found"
    );
    assert!(
        scripts.scripts_loaded > 0,
        "no scripts loaded from the data view"
    );
    let scenario = soe_game::harness::Scenario::parse("tick 0.1").unwrap();
    let harness = soe_game::harness::run(&mut game, &scenario);
    assert!(harness.passed());
    let coverage = harness.coverage.unwrap();
    let total = coverage.scripts.functions.len();
    let executed = coverage
        .scripts
        .functions
        .iter()
        .filter(|f| !f.objects.is_empty())
        .count();
    // The installed vanilla corpus contains tens of thousands of bodies and
    // startup executes hundreds. These are survey floors, not completion goals.
    assert!(total > 10_000);
    assert!(executed > 100 && executed < total);
    assert!(coverage.quests.iter().any(|q| q.summary.executed > 0));
    assert!(
        coverage
            .quests
            .iter()
            .all(|q| q.summary.executed <= q.summary.total)
    );
    eprintln!(
        "coverage: {executed}/{total} bodies, {} scripts, {} quests",
        coverage.scripts.scripts.len(),
        coverage.quests.len()
    );
    if let Some(path) = std::env::var_os("SOE_COVERAGE_REPORT") {
        std::fs::write(path, coverage.to_text()).unwrap();
    }
}
