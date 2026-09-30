// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    save::{DependencyChange, DependencyKind, DependencySource, SavedStateKind},
    Game,
};
use soe_papyrus::asm::ScriptBuilder;
use soe_save::{Compression, FileLocationTable, GlobalData, Header, Save, WString};

fn plugin(light: bool, content: &str) -> Plugin {
    let mut file = Plugin::new(FileHeader {
        flags: if light { flags::LIGHT_MASTER } else { 0 },
        ..Default::default()
    });
    file.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), content));
    file
}

fn game(entries: &[(&str, bool, &str)], scripts: &[(&str, &str)]) -> Game {
    let mut order = LoadOrder::new();
    for (name, light, content) in entries {
        order.add(name, plugin(*light, content)).unwrap();
    }
    let mut game = Game::new(order);
    for (name, body) in scripts {
        game.load_script(&ScriptBuilder::new(name, body).bytes())
            .unwrap();
    }
    game
}

fn empty_save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Prisoner"),
        player_level: 1,
        player_location: WString::default(),
        game_date: WString::default(),
        player_race: WString::default(),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 0.0,
        filetime: 0,
        screenshot_width: 0,
        screenshot_height: 0,
        compression: Compression::None,
        screenshot: vec![],
    };
    let mut bytes = vec![];
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    FileLocationTable {
        form_id_array_count_offset: at + 8,
        unknown_table_3_offset: at + 16,
        table_1_offset: at,
        table_2_offset: at,
        change_forms_offset: at,
        table_3_offset: at,
        table_1_count: 0,
        table_2_count: 0,
        table_3_count: 0,
        change_form_count: 0,
        unused: [0; 15],
    }
    .write(&mut bytes);
    GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    Save::parse(&bytes).unwrap()
}

fn papyrus_with_script(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1_u16.to_le_bytes()); // VM version
    out.extend_from_slice(&1_u32.to_le_bytes()); // string count
    WString::from_text(name).write(&mut out);
    out.extend_from_slice(&1_u32.to_le_bytes()); // script count
    out.extend_from_slice(&0_u32.to_le_bytes()); // script name
    out.extend_from_slice(&0_u32.to_le_bytes()); // parent name
    out.extend_from_slice(&0_u32.to_le_bytes()); // members
    out.extend_from_slice(&0_u32.to_le_bytes()); // instances
    out.extend_from_slice(&0_u32.to_le_bytes()); // references
    out.extend_from_slice(&0_u32.to_le_bytes()); // arrays
    out.extend_from_slice(&0_u32.to_le_bytes()); // runtime ID
    out.extend_from_slice(&0_u32.to_le_bytes()); // active scripts
    out.extend_from_slice(&0_u32.to_le_bytes()); // instance data
    out.extend_from_slice(&0_u32.to_le_bytes()); // reference data
    out.extend_from_slice(&0_u32.to_le_bytes()); // messages
    out.extend_from_slice(&0_u32.to_le_bytes()); // suspended stack 1
    out.extend_from_slice(&0_u32.to_le_bytes()); // suspended stack 2
    out.extend_from_slice(&0_u32.to_le_bytes()); // execution unknown
    out.extend_from_slice(&0_u32.to_le_bytes()); // unknown IDs
    out.extend_from_slice(&0_u32.to_le_bytes()); // unbinds
    out.extend_from_slice(&1_u16.to_le_bytes()); // save file version
    out
}

#[test]
fn sav_57_native_report_lists_all_dependency_changes_without_mutation() {
    let source = game(
        &[
            ("Changed.esp", false, "old"),
            ("Kind.esl", true, "kind"),
            ("Moved.esp", false, "moved"),
            ("Missing.esp", false, "missing"),
        ],
        &[
            ("OldScript", "Int Function Value()\n  Return 1\nEndFunction"),
            (
                "ChangedScript",
                "Int Function Value()\n  Return 1\nEndFunction",
            ),
        ],
    );
    let bytes = source.save_state().unwrap();
    let before = source.save_state().unwrap();
    let current = game(
        &[
            ("Moved.esp", false, "moved"),
            ("Changed.esp", false, "new"),
            ("Kind.esl", false, "kind"),
            ("Added.esp", false, "added"),
        ],
        &[
            (
                "ChangedScript",
                "Int Function Value()\n  Return 2\nEndFunction",
            ),
            ("NewScript", ""),
        ],
    );
    let current_before = current.save_state().unwrap();

    let report = current.preflight_dependencies(&bytes).unwrap();
    assert_eq!(report.source, DependencySource::Native);
    assert_eq!(report, current.preflight_dependencies(&bytes).unwrap());
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Missing.esp" && finding.change == DependencyChange::Missing
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Kind.esl" && finding.change == DependencyChange::KindChanged
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Changed.esp" && finding.change == DependencyChange::FingerprintChanged
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Moved.esp" && finding.change == DependencyChange::SlotChanged
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "OldScript"
            && finding.kind == DependencyKind::Script
            && finding.affects == SavedStateKind::Vm
            && finding.change == DependencyChange::Missing
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "ChangedScript"
            && finding.change == DependencyChange::FingerprintChanged
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Added.esp" && finding.change == DependencyChange::Added
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "NewScript" && finding.change == DependencyChange::Added
    }));
    assert_eq!(source.save_state().unwrap(), before);
    assert_eq!(current.save_state().unwrap(), current_before);
}

#[test]
fn sav_57_vanilla_report_marks_unknown_provenance_and_missing_dependencies() {
    let mut save = empty_save();
    save.plugins = ["Missing.esp", "Kind.esl"].map(WString::from_text).to_vec();
    save.light_plugins = vec![];
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus_with_script("MissingScript"),
    });
    let current = game(&[("Kind.esl", true, "changed kind")], &[]);

    let report = current.preflight_vanilla_dependencies(&save).unwrap();
    assert_eq!(report.source, DependencySource::Vanilla);
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Missing.esp" && finding.change == DependencyChange::Missing
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Kind.esl" && finding.change == DependencyChange::KindChanged
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "MissingScript"
            && finding.kind == DependencyKind::Script
            && finding.change == DependencyChange::Missing
    }));
    for identity in ["Missing.esp", "Kind.esl", "MissingScript"] {
        assert!(report.findings.iter().any(|finding| {
            finding.identity == identity && finding.change == DependencyChange::ProvenanceUnknown
        }));
    }
}
