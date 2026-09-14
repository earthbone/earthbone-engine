// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Loads the five base masters from a real install through `soe_esm`,
//! builds a `Game` and binds every `VMAD` script the load order names. When
//! a loose `Data/Scripts` folder exists its `.pex` files are loaded first,
//! so the `BindReport` reflects the vanilla scripts and not only the record
//! data. Gated on `SKYRIM_DATA` pointing at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-game --test real_files -- --nocapture
//! ```

use std::path::{Path, PathBuf};
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

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

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

    let scripts_dir = data.join("Scripts");
    if scripts_dir.is_dir() {
        let started = Instant::now();
        let mut files = Vec::new();
        walk(&scripts_dir, &mut files);
        let mut loaded = 0usize;
        let mut load_failures: Vec<String> = Vec::new();
        for p in &files {
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pex")) {
                let bytes = std::fs::read(p).unwrap();
                match game.load_script(&bytes) {
                    Ok(()) => loaded += 1,
                    Err(e) => load_failures.push(format!("{}: {e}", p.display())),
                }
            }
        }
        eprintln!(
            "loaded {loaded} scripts under {} in {:.1}s, {} failures",
            scripts_dir.display(),
            started.elapsed().as_secs_f32(),
            load_failures.len()
        );
        for f in load_failures.iter().take(40) {
            eprintln!("  {f}");
        }
    } else {
        eprintln!(
            "no script folder at {}, binding against the record data alone",
            scripts_dir.display()
        );
    }

    let report = game.bind();
    eprintln!(
        "bind: {} forms with scripts, {} scripts attached, {} scripts missing, {} properties set, {} properties missing, {} quests started",
        report.forms_with_scripts,
        report.scripts_attached,
        report.scripts_missing.len(),
        report.properties_set,
        report.properties_missing.len(),
        report.quests_started,
    );
    eprintln!("first 20 missing scripts:");
    for (script, form) in report.scripts_missing.iter().take(20) {
        eprintln!("  {script} on {form}");
    }
    eprintln!("first 20 missing properties:");
    for (script, prop) in report.properties_missing.iter().take(20) {
        eprintln!("  {script}.{prop}");
    }
    assert!(
        report.forms_with_scripts > 0,
        "no VMAD-carrying forms found"
    );
}
