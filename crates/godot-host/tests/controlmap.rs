// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The vanilla `interface/controlmap.txt`, read out of the shipped archive.
//!
//! These are the facts the parser was written against, asserted against the
//! real file rather than against a fixture, so a wrong belief about the
//! format fails here instead of surviving into the host.
//!
//! Gated on `SKYRIM_DATA`.

use soe_bsa::Archive;
use soe_godot_host::input::{Action, Bindings, ControlMap};
use std::path::PathBuf;

fn controlmap() -> Option<String> {
    let dir = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let path = dir.join("Skyrim - Interface.bsa");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    let archive = Archive::open(&path).expect("the interface archive opens");
    let name = archive
        .files()
        .iter()
        .find(|n| n.to_ascii_lowercase().ends_with("controlmap.txt"))
        .cloned()?;
    let bytes = archive.read(&name).expect("controlmap reads");
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
fn the_keyboard_column_holds_windows_virtual_key_codes() {
    let Some(text) = controlmap() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let map = ControlMap::parse(&text);

    // F1 through F12 are contiguous from 0x70 in the virtual key table and
    // nowhere near contiguous in DirectInput's. The debug overlay context
    // binds them by name, so the file says which table it uses.
    for (name, want) in [
        ("F1", 0x70_u32),
        ("F2", 0x71),
        ("F3", 0x72),
        ("F7", 0x76),
        ("F12", 0x7b),
        ("Backspace", 0x08),
        ("Enter", 0x0d),
    ] {
        let m = map.find(name).unwrap_or_else(|| panic!("{name} missing"));
        let got = &m.keyboard.0;
        assert_eq!(got.len(), 1, "{name} had {} chords", got.len());
        assert_eq!(got[0].0, vec![want], "{name} was not the virtual key code");
    }
}

#[test]
fn the_shipped_file_leaves_the_gameplay_keyboard_unmapped() {
    let Some(text) = controlmap() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let map = ControlMap::parse(&text);

    // This is why the engine carries its own keyboard defaults. Every
    // gameplay action is either absent from the file or bound on the gamepad
    // only, so applying the shipped map must change nothing.
    for action in Action::all() {
        match map.find(action.event_name()) {
            None => eprintln!("{:<20} absent from the file", action.event_name()),
            Some(m) => {
                eprintln!(
                    "{:<20} keyboard {}",
                    action.event_name(),
                    if m.keyboard.is_unmapped() {
                        "unmapped".to_string()
                    } else {
                        format!("{:?}", m.keyboard)
                    }
                );
                assert!(
                    m.keyboard.is_unmapped(),
                    "{} is bound on the keyboard after all, so the engine's \
                     default table is now wrong for it",
                    action.event_name()
                );
            }
        }
    }

    let mut bindings = Bindings::default();
    let before = bindings.clone();
    bindings.apply_control_map(&map);
    assert_eq!(
        bindings, before,
        "the shipped map disturbed the keyboard defaults"
    );
}

#[test]
fn no_movement_event_exists_in_the_shipped_file() {
    let Some(text) = controlmap() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let map = ControlMap::parse(&text);
    for absent in ["Forward", "Strafe Left", "Strafe Right"] {
        assert!(
            map.find(absent).is_none(),
            "{absent} exists after all, the engine should read it rather than \
             carry a default"
        );
    }
    // "Move" does exist, as the gamepad's analog stick.
    let move_event = map.find("Move").expect("Move is in the file");
    assert!(move_event.keyboard.is_unmapped());
    assert!(!move_event.gamepad.is_unmapped());
}
