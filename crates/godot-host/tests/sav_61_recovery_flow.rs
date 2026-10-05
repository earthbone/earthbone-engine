// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    save::{DependencyChange, DependencySource},
    save_generations::GenerationStore,
    Game,
};
use soe_save::{
    reference::PlayerLocation, ChangeForm, Compression, FileLocationTable, GlobalData, Header,
    RefId, Save, WString,
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

fn recoverable_save() -> Save {
    let mut save = empty_save();
    save.change_forms.push(ChangeForm {
        form: RefId::from_bytes([0x40, 0, 0x14]),
        change_flags: 2,
        type_field: 1,
        version: 1,
        uncompressed_len: 0,
        data: {
            let mut data = Vec::new();
            RefId::from_bytes([0x40, 0, 0x08]).write(&mut data);
            for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
                data.extend_from_slice(&value.to_le_bytes());
            }
            data.extend_from_slice(&[0; 8]);
            data
        },
    });
    save.table_1.push(GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF00_0804,
            world: RefId::from_bytes([0x40, 0, 0]),
            coordinates: [0, 0],
            cell_or_world: RefId::from_bytes([0x40, 0, 0x08]),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    let mut papyrus = Vec::new();
    papyrus.extend_from_slice(&1_u16.to_le_bytes());
    for _ in 0..16 {
        papyrus.extend_from_slice(&0_u32.to_le_bytes());
    }
    papyrus.extend_from_slice(&1_u16.to_le_bytes());
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus,
    });
    save
}

fn game_with_skyrim() -> Game {
    let mut skyrim = Plugin::new(FileHeader::default());
    skyrim.push_top(Record::new(Tag(*b"CELL"), 0x800));
    skyrim.push_top(Record::new(Tag(*b"NPC_"), 0x7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", skyrim).unwrap();
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
fn sav_19_transient_projectiles_are_counted_and_source_is_preserved() {
    let directory = Directory::new();
    let source_path = directory.0.join("unresolved.ess");
    let mut save = recoverable_save();
    for (index, type_field) in [47, 47, 40, 3].into_iter().enumerate() {
        save.change_forms.push(ChangeForm {
            form: RefId::from_bytes([0x80, 0, 0x20 + index as u8]),
            change_flags: 0,
            type_field,
            version: 1,
            uncompressed_len: 0,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        });
    }
    let source_bytes = save.write().unwrap();
    fs::write(&source_path, &source_bytes).unwrap();
    let before = save.write().unwrap();
    let mut game = game_with_skyrim();

    let accepted = soe_godot_host::save_recovery::accept_vanilla_recovery_session(
        &mut game,
        &save,
        &directory.store(),
    )
    .unwrap();
    assert_eq!(accepted.skipped_transient.get("PARW"), Some(&2));
    assert_eq!(accepted.skipped_transient.get("PHZD"), Some(&1));
    assert_eq!(accepted.skipped_transient.get("PGRE"), Some(&1));
    for object in 0x20..=0x23 {
        assert!(game
            .world()
            .reference_ref(soe_esm::GlobalFormId {
                file: u16::MAX,
                object,
            })
            .is_none());
    }
    assert_eq!(save.write().unwrap(), before);
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
}

#[test]
fn sav_19_transient_omission_does_not_bypass_atomic_restore_failure() {
    let directory = Directory::new();
    let store = directory.store();
    let mut save = recoverable_save();
    save.change_forms.push(ChangeForm {
        form: RefId::from_bytes([0x80, 0, 0x20]),
        change_flags: 0,
        type_field: 3,
        version: 1,
        uncompressed_len: 0,
        data: vec![],
    });
    save.table_1.retain(|table| table.kind != 1);
    let mut game = game_with_skyrim();
    let before = game.save_state().unwrap();
    assert!(matches!(
        soe_godot_host::save_recovery::accept_vanilla_recovery_session(&mut game, &save, &store),
        Err(soe_godot_host::save_recovery::VanillaRecoveryError::Restore { .. })
    ));
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(
        fs::read_dir(directory.0.join("recovery")).unwrap().count(),
        0
    );
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

#[test]
fn sav_61_host_accepts_recoverable_ess_to_distinct_generation_and_keeps_source() {
    let directory = Directory::new();
    let source_path = directory.0.join("accepted-source.ess");
    let save = recoverable_save();
    let source_bytes = save.write().unwrap();
    fs::write(&source_path, &source_bytes).unwrap();
    let recovery = directory.store();
    let mut game = game_with_skyrim();

    let accepted =
        soe_godot_host::save_recovery::accept_vanilla_recovery_session(&mut game, &save, &recovery)
            .unwrap();

    assert!(accepted
        .generation
        .path
        .starts_with(directory.0.join("recovery")));
    assert_ne!(accepted.generation.path, source_path);
    assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
    let recovery_files = fs::read_dir(directory.0.join("recovery"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "soeb")
        })
        .count();
    assert_eq!(recovery_files, 1);
}
