// SPDX-License-Identifier: GPL-3.0-or-later

//! The installed game, gated on `SKYRIM_DATA` pointing at its `Data` folder.
//! With no `plugins.txt` in reach, the engine loads the hardcoded masters and
//! the owned creations, and `Skyrim_Default.ini` names the archives.

use std::path::PathBuf;

use soe_profile::{Layer, LoadoutInput, Problem};

#[test]
fn the_installed_game_resolves_to_its_early_loaders_and_archives() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let game = data.parent().expect("Data has a parent").to_path_buf();
    let loadout = LoadoutInput::vanilla(&game, None, None).resolve().unwrap();
    let order = &loadout.load_order;
    for e in &order.entries {
        eprintln!(
            "{:>18?} {:<5} {}",
            e.slot,
            if e.early { "early" } else { "" },
            e.name
        );
    }
    for p in &order.problems {
        eprintln!("problem: {p:?}");
    }
    let active = order.active_names();
    assert!(active.len() >= 5);
    assert_eq!(
        &active[..5],
        &[
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm"
        ]
    );
    assert!(
        order.active().all(|e| e.early),
        "with no plugins.txt only the engine's own list is active"
    );
    assert!(!order.broken(), "{:?}", order.problems);
    assert!(
        order
            .problems
            .iter()
            .all(|p| !matches!(p, Problem::Unreadable { .. })),
        "{:?}",
        order.problems
    );
    let light: Vec<&str> = order
        .active()
        .filter(|e| e.light)
        .map(|e| e.name.as_str())
        .collect();
    eprintln!("light early loaders: {light:?}");

    assert_eq!(
        loadout.settings.get("General", "sLanguage"),
        Some("ENGLISH")
    );
    assert_eq!(
        loadout.settings.source("General", "sLanguage"),
        Some(&Layer::Default)
    );
    let names: Vec<&str> = loadout.archives.iter().map(|a| a.name.as_str()).collect();
    eprintln!("archives: {names:?}");
    assert_eq!(names[0], "Skyrim - Misc.bsa");
    assert!(names.contains(&"Skyrim - Textures0.bsa"));
    let unresolved: Vec<&str> = loadout
        .archives
        .iter()
        .filter(|a| a.path.is_none())
        .map(|a| a.name.as_str())
        .collect();
    // The shipped Skyrim_Default.ini names `Skyrim - Patch.bsa`, which this
    // install does not have. The engine skips an archive it cannot open, so
    // that one is the only tolerated gap.
    assert!(
        unresolved.iter().all(|n| *n == "Skyrim - Patch.bsa"),
        "archives named by the INI but not on disk: {unresolved:?}"
    );
    assert!(loadout.find("Skyrim - Misc.bsa").is_some());
}
