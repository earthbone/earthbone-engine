// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Slot};

fn plugin(light: bool, value: &str) -> Plugin {
    let mut file = Plugin::new(FileHeader {
        flags: if light { flags::LIGHT_MASTER } else { 0 },
        ..Default::default()
    });
    file.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, value));
    file
}

fn order(names: &[&str]) -> LoadOrder {
    let mut order = LoadOrder::new();
    for name in names {
        order.add(name, plugin(*name == "Light.esl", name)).unwrap();
    }
    order
}

#[test]
fn sav_67_reordering_regular_and_light_slots_retains_plugin_fingerprints() {
    let first = order(&["First.esp", "Light.esl", "Second.esp"]);
    let reordered = order(&["Light.esl", "Second.esp", "First.esp"]);
    for name in ["First.esp", "Light.esl", "Second.esp"] {
        let before = first.file(first.position_of(name).unwrap());
        let after = reordered.file(reordered.position_of(name).unwrap());
        assert_eq!(before.fingerprint, after.fingerprint, "{name}");
    }
    assert_ne!(
        first.file(first.position_of("First.esp").unwrap()).slot,
        reordered
            .file(reordered.position_of("First.esp").unwrap())
            .slot
    );
    assert!(matches!(
        reordered
            .file(reordered.position_of("Light.esl").unwrap())
            .slot,
        Slot::Light(_)
    ));
}

#[test]
fn sav_67_changed_loaded_plugin_definition_changes_fingerprint() {
    let original = Plugin::parse(&plugin(false, "Original").to_bytes()).unwrap();
    let replacement = Plugin::parse(&plugin(false, "Replacement").to_bytes()).unwrap();
    let mut first = LoadOrder::new();
    let mut second = LoadOrder::new();
    first.add("Same.esp", original).unwrap();
    second.add("Same.esp", replacement).unwrap();
    assert_ne!(first.file(0).fingerprint, second.file(0).fingerprint);
}

#[test]
fn sav_67_loaded_plugin_fingerprint_ignores_engine_added_global() {
    let file = plugin(false, "Original");
    let expected = file.fingerprint();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", file).unwrap();
    assert_eq!(order.file(0).fingerprint, expected);
}
