// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The vanilla PC control map, read out of the shipped archive.
//!
//! These are the facts the parser was written against, asserted against the
//! real file rather than against a fixture, so a wrong belief about the
//! format fails here instead of surviving into the host.
//!
//! Gated on `SKYRIM_DATA`.

use soe_bsa::Archive;
use soe_godot_host::input::{Action, Binding, Bindings, ControlMap};
use std::path::PathBuf;

fn controlmap() -> Option<String> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let path = dir.join("Skyrim - Interface.bsa");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    let archive = Archive::open(&path).expect("the interface archive opens");
    let bytes = archive
        .read("interface/controls/pc/controlmap.txt")
        .expect("the PC control map reads");
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[test]
fn the_shipped_control_map_parses() {
    let Some(text) = controlmap() else {
        eprintln!("skipping: SKYRIM_DATA is not set or the file is absent");
        return;
    };
    let map = ControlMap::parse(&text);

    eprintln!("controlmap.txt: {} bytes", text.len());
    eprintln!("  contexts {}", map.contexts.len());
    eprintln!("  mappings {}", map.mapping_count());
    for c in &map.contexts {
        eprintln!(
            "    {:<20} {} mapping(s)",
            c.name.as_deref().unwrap_or("(unnamed)"),
            c.mappings.len()
        );
    }

    assert!(map.contexts.len() > 5, "contexts: {}", map.contexts.len());
    assert!(
        map.mapping_count() > 50,
        "mappings: {}",
        map.mapping_count()
    );

    // Every line that is not a comment or blank must have parsed. A silent
    // skip would look identical to a clean parse without this.
    let body_lines = text
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("//")
        })
        .count();
    assert_eq!(
        body_lines,
        map.mapping_count(),
        "some lines were skipped by the parser"
    );

    // The first context is Main Gameplay and it names itself.
    let gameplay = map.gameplay().expect("a first context");
    assert_eq!(gameplay.name.as_deref(), Some("Main Gameplay"));
    for want in ["Activate", "Jump", "Sneak", "Toggle POV", "Sprint"] {
        assert!(gameplay.find(want).is_some(), "{want} missing");
    }
}

#[test]
fn the_pc_keyboard_column_holds_directinput_scan_codes() {
    let Some(text) = controlmap() else { return };
    let map = ControlMap::parse(&text);
    for (name, want) in [
        ("F1", 0x3b_u32),
        ("F2", 0x3c),
        ("F3", 0x3d),
        ("F7", 0x41),
        ("F12", 0x58),
        ("Backspace", 0x0e),
        ("Enter", 0x1c),
    ] {
        let mapping = map.find(name).unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            mapping.keyboard,
            Binding::key(want),
            "{name}: wrong PC key table"
        );
    }
}

#[test]
fn the_pc_gameplay_bindings_reach_the_input_driver() {
    let Some(text) = controlmap() else { return };
    let map = ControlMap::parse(&text);
    let gameplay = map.gameplay().expect("gameplay context");
    let mut bindings = Bindings::default();
    bindings.apply_control_map(&map);
    for (action, code) in [
        (Action::Forward, 0x11),
        (Action::Back, 0x1f),
        (Action::StrafeLeft, 0x1e),
        (Action::StrafeRight, 0x20),
        (Action::Activate, 0x12),
        (Action::Jump, 0x39),
        (Action::Sneak, 0x1d),
        (Action::Sprint, 0x38),
        (Action::Walk, 0x2a),
        (Action::ToggleAlwaysRun, 0x3a),
        (Action::TogglePov, 0x21),
        (Action::ReadyWeapon, 0x13),
    ] {
        let name = action.event_name();
        let mapping = gameplay
            .find(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            mapping.keyboard,
            Binding::key(code),
            "{name}: unexpected shipped PC binding"
        );
        assert_eq!(
            bindings.get(action),
            Some(&mapping.keyboard),
            "{name}: overlay was not applied"
        );
    }
}

#[test]
fn menu_bindings_do_not_supply_missing_gameplay_actions() {
    let map = ControlMap::parse(
        "// Main Gameplay\nJump\t0x39\t0xff\t0xff\t1\t1\t1\n\n// Menu\nActivate\t0x14\t0xff\t0xff\t1\t1\t1",
    );
    let mut bindings = Bindings::default();
    bindings.apply_control_map(&map);
    assert_eq!(bindings.get(Action::Activate), Some(&Binding::key(0x12)));
}
