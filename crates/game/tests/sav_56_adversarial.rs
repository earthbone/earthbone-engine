// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record};
use soe_game::{save::DependencyChange, save::DependencyKind, save::SavedStateKind, Game};
use soe_papyrus::asm::ScriptBuilder;

fn plugin(light: bool, contents: &str) -> Plugin {
    let mut file = Plugin::new(FileHeader {
        flags: if light { flags::LIGHT_MASTER } else { 0 },
        ..Default::default()
    });
    file.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, contents));
    file
}

fn game(entries: &[(&str, bool, &str)], scripts: &[(&str, &str)]) -> Game {
    let mut order = LoadOrder::new();
    for (name, light, contents) in entries {
        order.add(name, plugin(*light, contents)).unwrap();
    }
    let mut game = Game::new(order);
    for (name, body) in scripts {
        game.load_script(&ScriptBuilder::new(name, body).bytes())
            .unwrap();
    }
    game
}

fn finding(game: &Game, bytes: &[u8], kind: DependencyKind, identity: &str, change: DependencyChange) -> bool {
    game.preflight_dependencies(bytes)
        .unwrap()
        .findings
        .iter()
        .any(|item| item.kind == kind && item.identity.eq_ignore_ascii_case(identity) && item.change == change)
}

#[test]
fn sav_56_regular_and_light_slot_zero_remain_distinct() {
    let source = game(
        &[("Regular.esp", false, "regular"), ("Light.esl", true, "light")],
        &[("Fixture", "Int Function Value()\n  Return 1\nEndFunction")],
    );
    let bytes = source.save_state().unwrap();
    let saved = Game::saved_provenance(&bytes).unwrap();
    let regular = saved.plugins.iter().find(|plugin| plugin.name == "Regular.esp").unwrap();
    let light = saved.plugins.iter().find(|plugin| plugin.name == "Light.esl").unwrap();

    assert!(!regular.light);
    assert!(light.light);
    assert_eq!(regular.slot, 0);
    assert_eq!(light.slot, 0);
    let mut restored = game(
        &[("Regular.esp", false, "regular"), ("Light.esl", true, "light")],
        &[("Fixture", "Int Function Value()\n  Return 1\nEndFunction")],
    );
    restored.load_state(&bytes).unwrap();
    assert_eq!(Game::saved_provenance(&restored.save_state().unwrap()).unwrap(), saved);
}

#[test]
fn sav_56_light_slot_movement_keeps_content_identity_and_is_reported() {
    let source = game(
        &[("LightA.esl", true, "a"), ("LightB.esl", true, "b")],
        &[],
    );
    let bytes = source.save_state().unwrap();
    let saved = Game::saved_provenance(&bytes).unwrap();
    let mut reordered = game(
        &[("LightB.esl", true, "b"), ("LightA.esl", true, "a")],
        &[],
    );
    let current = Game::saved_provenance(&reordered.save_state().unwrap()).unwrap();
    let old_a = saved.plugins.iter().find(|plugin| plugin.name == "LightA.esl").unwrap();
    let new_a = current.plugins.iter().find(|plugin| plugin.name == "LightA.esl").unwrap();

    assert_eq!(old_a.fingerprint, new_a.fingerprint);
    assert_ne!(old_a.slot, new_a.slot);
    assert!(finding(&reordered, &bytes, DependencyKind::Plugin, "LightA.esl", DependencyChange::SlotChanged));
    assert!(reordered.load_state(&bytes).is_err());
}

#[test]
fn sav_56_same_name_plugin_replacement_reports_changed_fingerprint() {
    let source = game(&[("Patch.esp", false, "old")], &[]);
    let bytes = source.save_state().unwrap();
    let mut replacement = game(&[("Patch.esp", false, "new")], &[]);

    assert!(finding(&replacement, &bytes, DependencyKind::Plugin, "Patch.esp", DependencyChange::FingerprintChanged));
    assert!(replacement.load_state(&bytes).unwrap_err().to_string().contains("content changed"));
}

#[test]
fn sav_56_same_name_plugin_cannot_change_regular_light_kind() {
    let source = game(&[("Patch.esp", false, "same")], &[]);
    let bytes = source.save_state().unwrap();
    let mut relabeled = game(&[("Patch.esp", true, "same")], &[]);

    assert!(finding(&relabeled, &bytes, DependencyKind::Plugin, "Patch.esp", DependencyChange::KindChanged));
    assert!(relabeled.load_state(&bytes).unwrap_err().to_string().contains("regular/light kind"));
}

#[test]
fn sav_56_changed_script_is_reported_and_failed_load_leaves_state_unchanged() {
    let body = "Int Function Value()\n  Return 1\nEndFunction";
    let source = game(&[], &[("Fixture", body)]);
    let bytes = source.save_state().unwrap();
    let mut changed = game(&[], &[("Fixture", "Int Function Value()\n  Return 2\nEndFunction")]);
    let before = changed.save_state().unwrap();

    assert!(finding(&changed, &bytes, DependencyKind::Script, "Fixture", DependencyChange::FingerprintChanged));
    assert!(changed.load_state(&bytes).unwrap_err().to_string().contains("script fingerprints differ"));
    assert_eq!(changed.save_state().unwrap(), before);
}

#[test]
fn sav_56_missing_script_is_reported_as_vm_state_dependency() {
    let source = game(&[], &[("Fixture", "Int Function Value()\n  Return 1\nEndFunction")]);
    let bytes = source.save_state().unwrap();
    let mut missing = game(&[], &[]);
    let report = missing.preflight_dependencies(&bytes).unwrap();

    assert!(report.findings.iter().any(|item| {
        item.kind == DependencyKind::Script
            && item.identity == "Fixture"
            && item.affects == SavedStateKind::Vm
            && item.change == DependencyChange::Missing
    }));
    assert!(missing.load_state(&bytes).is_err());
}
