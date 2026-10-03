// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::{save_generations::GenerationStore, Game};
use soe_save::{ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString};
use std::{fs, path::PathBuf};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap().keep())
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
fn sav_61_host_refuses_unimported_active_projectile_state_before_restoration() {
    let directory = Directory::new();
    let store = directory.store();
    let source_path = directory.0.join("source.ess");
    let mut missed_refusals = Vec::new();

    // UESP's save type table identifies indices 2 to 5 and 40 to 41 as
    // active projectile or hazard change forms. CommonLibSSE's FormTypes.h
    // confirms their runtime types. The vanilla restore transaction does
    // not import these forms, so recovery must not silently accept and drop them.
    for (type_field, expected_type) in [
        (2, "PMIS"),
        (3, "PGRE"),
        (4, "PBEA"),
        (5, "PFLA"),
        (40, "PHZD"),
        (41, "PBAR"),
    ] {
        let mut save = empty_save();
        save.change_forms.push(ChangeForm {
            form: RefId::from_bytes([0, 0, 0]),
            change_flags: 0,
            type_field,
            version: 1,
            uncompressed_len: 0,
            data: vec![0xD1, 0xA6],
        });
        let source_bytes = save.write().unwrap();
        fs::write(&source_path, &source_bytes).unwrap();
        let mut game = game_with_skyrim();
        let before = game.save_state().unwrap();

        let error = soe_godot_host::save_recovery::accept_vanilla_recovery_session(
            &mut game, &save, &store,
        )
        .unwrap_err();

        match error {
            soe_godot_host::save_recovery::VanillaRecoveryError::Refused { reasons, .. } => {
                assert!(reasons.iter().any(|reason| reason.contains(expected_type)));
            }
            other => missed_refusals.push(format!("{expected_type}: {other:?}")),
        }
        assert_eq!(game.save_state().unwrap(), before);
        assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
        assert_eq!(fs::read_dir(directory.0.join("recovery")).unwrap().count(), 0);
    }
    assert!(
        missed_refusals.is_empty(),
        "unimported projectile state was not refused: {missed_refusals:?}"
    );
}
