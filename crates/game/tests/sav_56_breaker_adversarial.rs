#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record};
use soe_game::{
    save::{DependencyChange, DependencyKind},
    Game,
};
use soe_papyrus::asm::ScriptBuilder;

fn plugin() -> Plugin {
    let mut file = Plugin::new(FileHeader::default());
    file.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "content"));
    file
}

fn game(entries: &[&str]) -> Game {
    let mut order = LoadOrder::new();
    for name in entries {
        order.add(name, plugin()).unwrap();
    }
    Game::new(order)
}

#[test]
fn sav_56_regular_plugin_slot_move_keeps_fingerprint_and_reports_only_slot_change() {
    let source = game(&["First.esp", "Second.esp"]);
    let bytes = source.save_state().unwrap();
    let previous = Game::saved_provenance(&bytes).unwrap();
    let reordered = game(&["Second.esp", "First.esp"]);
    let current = Game::saved_provenance(&reordered.save_state().unwrap()).unwrap();
    let old = previous
        .plugins
        .iter()
        .find(|p| p.name == "First.esp")
        .unwrap();
    let now = current
        .plugins
        .iter()
        .find(|p| p.name == "First.esp")
        .unwrap();

    assert_eq!(old.fingerprint, now.fingerprint);
    assert_ne!(old.slot, now.slot);
    let report = reordered.preflight_dependencies(&bytes).unwrap();
    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Plugin
            && finding.identity == "First.esp"
            && finding.change == DependencyChange::SlotChanged
    }));
    assert!(!report.findings.iter().any(|finding| {
        finding.identity == "First.esp" && finding.change == DependencyChange::FingerprintChanged
    }));
}

#[test]
fn sav_56_added_plugin_is_reported_without_changing_saved_plugin_fingerprints() {
    let source = game(&["Base.esp"]);
    let bytes = source.save_state().unwrap();
    let with_added = game(&["Base.esp", "New.esp"]);
    let report = with_added.preflight_dependencies(&bytes).unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Plugin
            && finding.identity == "New.esp"
            && finding.change == DependencyChange::Added
    }));
    assert!(!report.findings.iter().any(|finding| {
        finding.identity == "Base.esp" && finding.change == DependencyChange::FingerprintChanged
    }));
}

#[test]
fn sav_56_script_fingerprint_round_trips_and_changed_source_is_reported() {
    let mut source = game(&[]);
    source
        .load_script(
            &ScriptBuilder::new("Fixture", "Int Function V()\n Return 1\nEndFunction").bytes(),
        )
        .unwrap();
    let bytes = source.save_state().unwrap();
    let saved = Game::saved_provenance(&bytes).unwrap();
    assert_eq!(saved.scripts.len(), 1);
    assert!(saved.scripts[0].fingerprint.is_some());

    let mut matching = game(&[]);
    matching
        .load_script(
            &ScriptBuilder::new("Fixture", "Int Function V()\n Return 1\nEndFunction").bytes(),
        )
        .unwrap();
    matching.load_state(&bytes).unwrap();
    assert_eq!(
        Game::saved_provenance(&matching.save_state().unwrap()).unwrap(),
        saved
    );

    let mut changed = game(&[]);
    changed
        .load_script(
            &ScriptBuilder::new("Fixture", "Int Function V()\n Return 2\nEndFunction").bytes(),
        )
        .unwrap();
    assert!(changed
        .preflight_dependencies(&bytes)
        .unwrap()
        .findings
        .iter()
        .any(|finding| {
            finding.kind == DependencyKind::Script
                && finding.identity == "Fixture"
                && finding.change == DependencyChange::FingerprintChanged
        }));
}

#[test]
fn sav_56_light_plugin_identity_is_preserved_in_native_provenance() {
    let mut light = Plugin::new(FileHeader {
        flags: flags::LIGHT_MASTER,
        ..Default::default()
    });
    light.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "light"));
    let mut order = LoadOrder::new();
    order.add("Light.esl", light).unwrap();
    let original = Game::new(order);
    let bytes = original.save_state().unwrap();
    let provenance = Game::saved_provenance(&bytes).unwrap();
    let item = provenance
        .plugins
        .iter()
        .find(|plugin| plugin.name == "Light.esl")
        .unwrap();
    assert!(item.light);
    assert_eq!(item.slot, 0);

    let mut order = LoadOrder::new();
    let mut matching = Plugin::new(FileHeader {
        flags: flags::LIGHT_MASTER,
        ..Default::default()
    });
    matching.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "light"));
    order.add("Light.esl", matching).unwrap();
    let mut loaded = Game::new(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(
        Game::saved_provenance(&loaded.save_state().unwrap()).unwrap(),
        provenance
    );
}

#[test]
fn sav_56_same_name_replacement_is_fingerprint_change_with_stable_slot() {
    let source = game(&["Patch.esp"]);
    let bytes = source.save_state().unwrap();
    let mut replacement = Plugin::new(FileHeader::default());
    replacement.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "replacement"));
    let mut order = LoadOrder::new();
    order.add("Patch.esp", replacement).unwrap();
    let changed = Game::new(order);
    let report = changed.preflight_dependencies(&bytes).unwrap();
    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Plugin
            && finding.identity == "Patch.esp"
            && finding.change == DependencyChange::FingerprintChanged
    }));
    assert!(!report.findings.iter().any(|finding| {
        finding.identity == "Patch.esp" && finding.change == DependencyChange::SlotChanged
    }));
}
