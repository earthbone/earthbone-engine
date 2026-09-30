// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{
    save::{DependencyChange, DependencyKind, SavedStateKind},
    Game,
};
use soe_save::{Compression, FileLocationTable, GlobalData, Header, Save, WString};

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

fn empty_papyrus() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1_u16.to_le_bytes()); // VM version
    out.extend_from_slice(&0_u32.to_le_bytes()); // strings
    out.extend_from_slice(&0_u32.to_le_bytes()); // scripts
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

fn add_script_list_tail(papyrus: &mut Vec<u8>, scripts: &[&str]) {
    // UESP Papyrus.wikitext places five auxiliary arrays before ScriptList.
    for _ in 0..5 {
        papyrus.extend_from_slice(&0_u32.to_le_bytes());
    }
    papyrus.extend_from_slice(&(scripts.len() as u32).to_le_bytes());
    for script in scripts {
        papyrus.extend_from_slice(&(script.len() as u32).to_le_bytes());
        papyrus.extend_from_slice(script.as_bytes());
    }
}

#[test]
fn sav_57_preflight_reports_missing_script_from_uesp_script_list_tail() {
    let mut save = empty_save();
    let mut papyrus = empty_papyrus();
    add_script_list_tail(&mut papyrus, &["MissingFromScriptList"]);
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus,
    });
    let game = Game::new(LoadOrder::new());

    let report = game.preflight_vanilla_dependencies(&save).unwrap();
    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Script
            && finding.identity == "MissingFromScriptList"
            && finding.affects == SavedStateKind::Vm
            && finding.change == DependencyChange::Missing
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Script
            && finding.identity == "MissingFromScriptList"
            && finding.change == DependencyChange::ProvenanceUnknown
    }));
}

#[test]
fn sav_57_malformed_papyrus_string_index_errors_before_mutation() {
    let game = Game::new(LoadOrder::new());
    let before = game.save_state().unwrap();
    let mut save = empty_save();
    let mut papyrus = empty_papyrus();
    // Add one script definition whose name points outside the empty string table.
    papyrus[6..10].copy_from_slice(&1_u32.to_le_bytes()); // script count
    papyrus.splice(10..10, [0_u8; 12]); // invalid name and parent, no members
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus,
    });

    assert!(game.preflight_vanilla_dependencies(&save).is_err());
    assert_eq!(game.save_state().unwrap(), before);
}
