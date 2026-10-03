// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{
    save::{DependencyChange, DependencySource},
    save_generations::GenerationStore,
    Game,
};
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
};
use std::{fs, path::PathBuf};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.keep();
        Self(path)
    }

    fn store(&self) -> GenerationStore {
        let path = self.0.join("recovery");
        fs::create_dir(&path).unwrap();
        GenerationStore::new(path, 2).unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
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
        kind: GlobalData::GLOBAL_VARIABLES,
        data: vec![0],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.table_1.push(GlobalData {
        kind: GlobalData::GLOBAL_VARIABLES,
        data: vec![0],
    });
    save
}

fn game_with_skyrim() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

#[test]
fn sav_61_host_reports_missing_dependency_before_refusal_and_keeps_source() {
    let directory = Directory::new();
    let source_path = directory.0.join("source.ess");
    let source_bytes = b"original ESS bytes";
    fs::write(&source_path, source_bytes).unwrap();
    let mut save = empty_save();
    save.plugins.push(WString::from_text("Missing.esp"));
    let mut game = game_with_skyrim();
    let before = game.save_state().unwrap();

    let error = soe_godot_host::save_recovery::accept_vanilla_recovery_session(
        &mut game,
        &save,
        &directory.store(),
    )
    .unwrap_err();

    let soe_godot_host::save_recovery::VanillaRecoveryError::Refused { report, reasons } = error
    else {
        panic!("missing dependency should refuse session start")
    };
    assert_eq!(report.source, DependencySource::Vanilla);
    assert!(report.findings.iter().any(|finding| {
        finding.identity == "Missing.esp" && finding.change == DependencyChange::Missing
    }));
    assert!(reasons.iter().any(|reason| reason.contains("Missing.esp")));
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
}

#[test]
fn sav_61_host_refuses_unresolved_parw_and_preserves_ess_payload() {
    let directory = Directory::new();
    let source_path = directory.0.join("unresolved.ess");
    let mut save = empty_save();
    save.change_forms.push(ChangeForm {
        form: RefId::from_bytes([0, 0, 0]),
        change_flags: 0,
        type_field: 47,
        version: 1,
        uncompressed_len: 0,
        data: vec![0xDE, 0xAD, 0xBE, 0xEF],
    });
    let source_bytes = save.write().unwrap();
    fs::write(&source_path, &source_bytes).unwrap();
    let before = save.change_forms[0].data.clone();
    let mut game = game_with_skyrim();

    let error = soe_godot_host::save_recovery::accept_vanilla_recovery_session(
        &mut game,
        &save,
        &directory.store(),
    )
    .unwrap_err();

    let soe_godot_host::save_recovery::VanillaRecoveryError::Refused { reasons, .. } = error else {
        panic!("opaque PARW state should refuse session start")
    };
    assert!(reasons
        .iter()
        .any(|reason| reason.contains("PARW change form")));
    assert_eq!(save.change_forms[0].data, before);
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
}

#[test]
fn sav_61_host_reports_dependencies_before_refusing_missing_player_state() {
    let directory = Directory::new();
    let source_path = directory.0.join("source.ess");
    let source_bytes = b"original vanilla ESS source fixture";
    fs::write(&source_path, source_bytes).unwrap();
    let save = empty_save();
    let store = directory.store();
    let mut game = game_with_skyrim();

    let error =
        soe_godot_host::save_recovery::accept_vanilla_recovery_session(&mut game, &save, &store)
            .unwrap_err();

    let soe_godot_host::save_recovery::VanillaRecoveryError::Restore { report, detail } = error
    else {
        panic!("missing required player state should refuse session start")
    };
    assert_eq!(report.source, DependencySource::Vanilla);
    assert!(detail.contains("missing player location"));
    assert_eq!(
        fs::read_dir(directory.0.join("recovery")).unwrap().count(),
        0
    );
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
}
