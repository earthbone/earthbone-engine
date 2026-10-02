// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{
    Game,
    save::{DependencyChange, DependencyKind, SavedStateKind},
};
use soe_papyrus::asm::ScriptBuilder;
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

fn papyrus_with_child_parent(parent: &str, child: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&2_u32.to_le_bytes());
    WString::from_text(parent).write(&mut out);
    WString::from_text(child).write(&mut out);
    out.extend_from_slice(&1_u32.to_le_bytes()); // one script definition
    out.extend_from_slice(&1_u32.to_le_bytes()); // child name
    out.extend_from_slice(&0_u32.to_le_bytes()); // parent name
    out.extend_from_slice(&0_u32.to_le_bytes()); // members
    for _ in 0..15 {
        out.extend_from_slice(&0_u32.to_le_bytes());
    }
    out.extend_from_slice(&1_u16.to_le_bytes());
    out
}

fn game_with_script(name: &str) -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    game.load_script(&ScriptBuilder::new(name, "").bytes())
        .unwrap();
    game
}

#[test]
fn sav_57_reports_missing_parent_script_dependency() {
    let mut save = empty_save();
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus_with_child_parent("MissingParent", "KnownChild"),
    });
    let game = game_with_script("KnownChild");

    let report = game.preflight_vanilla_dependencies(&save).unwrap();
    assert!(report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Script
            && finding.identity == "MissingParent"
            && finding.affects == SavedStateKind::Vm
            && finding.change == DependencyChange::Missing
    }));
}

#[test]
fn sav_57_vanilla_script_matching_ignores_ascii_case() {
    let mut save = empty_save();
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus_with_child_parent("MIXEDCASEPARENT", "MIXEDCASECHILD"),
    });
    let mut game = game_with_script("mixedcasechild");
    game.load_script(&ScriptBuilder::new("mixedcaseparent", "").bytes())
        .unwrap();

    let report = game.preflight_vanilla_dependencies(&save).unwrap();
    assert!(!report.findings.iter().any(|finding| {
        finding.kind == DependencyKind::Script
            && matches!(
                finding.identity.as_str(),
                "MIXEDCASECHILD" | "MIXEDCASEPARENT"
            )
            && finding.change == DependencyChange::Missing
    }));
}
