// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{save_generations::GenerationStore, Game};
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
};
use std::fs;

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

#[test]
fn sav_61_reports_opaque_pcon_before_refusing_missing_dependency() {
    let directory = tempfile::tempdir().unwrap();
    let recovery = directory.path().join("recovery");
    fs::create_dir(&recovery).unwrap();
    let store = GenerationStore::new(recovery.clone(), 2).unwrap();
    let source = directory.path().join("source.ess");
    let mut save = empty_save();
    save.change_forms.push(ChangeForm {
        form: RefId::from_bytes([0, 0, 0]),
        change_flags: 0,
        type_field: 42,
        version: 1,
        uncompressed_len: 0,
        data: vec![0xC0, 0xDE],
    });
    save.plugins.push(WString::from_text("Missing.esp"));
    let source_bytes = save.write().unwrap();
    fs::write(&source, &source_bytes).unwrap();
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let before = game.save_state().unwrap();

    let result =
        soe_godot_host::save_recovery::accept_vanilla_recovery_session(&mut game, &save, &store);

    let Err(soe_godot_host::save_recovery::VanillaRecoveryError::Refused { reasons, .. }) = &result
    else {
        panic!("missing plugin should refuse before restore, observed: {result:?}");
    };
    assert!(
        reasons.iter().any(|reason| reason.contains("PCON")),
        "refusal should identify opaque PCON state, observed: {reasons:?}"
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read_dir(recovery).unwrap().count(), 0);
}
