// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record};
use soe_game::{
    save::{DependencyChange, SaveProvenance},
    Game,
};
use soe_papyrus::asm::ScriptBuilder;

fn plugin(light: bool, content: &str) -> Plugin {
    let mut file = Plugin::new(FileHeader {
        flags: if light { flags::LIGHT_MASTER } else { 0 },
        ..Default::default()
    });
    file.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, content));
    file
}

fn game(entries: &[(&str, bool, &str)], script_body: &str) -> Game {
    let mut order = LoadOrder::new();
    for (name, light, content) in entries {
        order.add(name, plugin(*light, content)).unwrap();
    }
    let mut game = Game::new(order);
    game.load_script(&ScriptBuilder::new("ProvenanceFixture", script_body).bytes())
        .unwrap();
    game
}

fn saved_provenance(game: &Game) -> SaveProvenance {
    Game::saved_provenance(&game.save_state().unwrap()).unwrap()
}

#[test]
fn sav_56_round_trip_records_regular_light_and_script_provenance() {
    let source = game(
        &[
            ("First.esp", false, "First"),
            ("Light.esl", true, "Light"),
            ("Second.esp", false, "Second"),
        ],
        "Int Function Value()\n  Return 1\nEndFunction",
    );
    let bytes = source.save_state().unwrap();
    let provenance = Game::saved_provenance(&bytes).unwrap();

    assert_eq!(provenance.plugins.len(), 3);
    assert!(provenance.plugins.iter().any(|plugin| {
        plugin.name == "First.esp" && !plugin.light && plugin.fingerprint != [0; 32]
    }));
    assert!(provenance.plugins.iter().any(|plugin| {
        plugin.name == "Light.esl" && plugin.light && plugin.fingerprint != [0; 32]
    }));
    assert_eq!(provenance.scripts.len(), 1);
    assert_eq!(provenance.scripts[0].name, "ProvenanceFixture");
    assert!(provenance.scripts[0].fingerprint.is_some());

    let mut restored = game(
        &[
            ("First.esp", false, "First"),
            ("Light.esl", true, "Light"),
            ("Second.esp", false, "Second"),
        ],
        "Int Function Value()\n  Return 1\nEndFunction",
    );
    restored.load_state(&bytes).unwrap();
    assert_eq!(saved_provenance(&restored), provenance);
}

#[test]
fn sav_56_slot_reordering_is_distinct_from_same_name_replacement() {
    let source = game(
        &[
            ("First.esp", false, "First"),
            ("Light.esl", true, "Light"),
            ("Second.esp", false, "Second"),
        ],
        "",
    );
    let bytes = source.save_state().unwrap();
    let saved = Game::saved_provenance(&bytes).unwrap();

    let mut reordered = game(
        &[
            ("Second.esp", false, "Second"),
            ("First.esp", false, "First"),
            ("Light.esl", true, "Light"),
        ],
        "",
    );
    let current = saved_provenance(&reordered);
    let saved_second = saved
        .plugins
        .iter()
        .find(|p| p.name == "Second.esp")
        .unwrap();
    let moved_second = current
        .plugins
        .iter()
        .find(|p| p.name == "Second.esp")
        .unwrap();
    assert_eq!(saved_second.fingerprint, moved_second.fingerprint);
    assert_ne!(saved_second.slot, moved_second.slot);
    assert!(reordered
        .load_state(&bytes)
        .unwrap_err()
        .to_string()
        .contains("moved load-order slot"));

    let mut replacement = game(
        &[
            ("First.esp", false, "First"),
            ("Light.esl", true, "Light"),
            ("Second.esp", false, "Replacement"),
        ],
        "",
    );
    let changed = saved_provenance(&replacement);
    let changed_second = changed
        .plugins
        .iter()
        .find(|p| p.name == "Second.esp")
        .unwrap();
    assert_eq!(saved_second.slot, changed_second.slot);
    assert_ne!(saved_second.fingerprint, changed_second.fingerprint);
    assert!(replacement
        .load_state(&bytes)
        .unwrap_err()
        .to_string()
        .contains("content changed"));
}

#[test]
fn sav_56_script_replacement_is_reported_and_suspended() {
    let mut source = game(&[], "Int Function Value()\n  Return 1\nEndFunction");
    let object = source.vm.create_object("saved script owner");
    source.vm.attach(object, "ProvenanceFixture").unwrap();
    let bytes = source.save_state().unwrap();
    let mut replacement = game(&[], "Int Function Value()\n  Return 2\nEndFunction");
    let saved = Game::saved_provenance(&bytes).unwrap();
    let current = saved_provenance(&replacement);
    assert_eq!(saved.scripts[0].name, current.scripts[0].name);
    assert_ne!(saved.scripts[0].fingerprint, current.scripts[0].fingerprint);
    let report = replacement.preflight_dependencies(&bytes).unwrap();
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "ProvenanceFixture"
            && finding.change == DependencyChange::FingerprintChanged
    }));
    replacement.load_state(&bytes).unwrap();
    assert!(replacement.vm.has_suspended_state());
    assert_eq!(replacement.vm.run(100), 0);
}

#[test]
fn sav_56_unknown_native_version_is_rejected() {
    let mut source = game(&[], "");
    let mut bytes = source.save_state().unwrap();
    bytes[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
    let before = source.save_state().unwrap();
    assert!(source
        .load_state(&bytes)
        .unwrap_err()
        .to_string()
        .contains("game save version 65535"));
    assert_eq!(source.save_state().unwrap(), before);
}
