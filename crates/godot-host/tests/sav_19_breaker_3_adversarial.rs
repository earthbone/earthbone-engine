// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Opaque projectile payloads must not be accepted as disposable state.

use std::{fs, path::PathBuf};

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{save_generations::GenerationStore, Game};
use soe_save::{
    reference::PlayerLocation, ChangeForm, Compression, FileLocationTable, GlobalData, Header,
    RefId, Save, WString,
};

// Keep the repository's real-save-to-windowed-host check in this targeted
// adversarial binary so it exercises the done check with the provided corpus.
#[path = "sav_19_vanilla_attach.rs"]
mod vanilla_attach_done_check;

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

fn recoverable_save(type_field: u8) -> Save {
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
    let table_at = bytes.len() as u32 + 100;
    FileLocationTable {
        form_id_array_count_offset: table_at + 8,
        unknown_table_3_offset: table_at + 16,
        table_1_offset: table_at,
        table_2_offset: table_at,
        change_forms_offset: table_at,
        table_3_offset: table_at,
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
    save.table_3.push(GlobalData {
        kind: GlobalData::PAPYRUS,
        data: {
            let mut papyrus = Vec::new();
            papyrus.extend_from_slice(&1_u16.to_le_bytes());
            for _ in 0..16 {
                papyrus.extend_from_slice(&0_u32.to_le_bytes());
            }
            papyrus.extend_from_slice(&1_u16.to_le_bytes());
            papyrus
        },
    });
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
    save.change_forms.push(ChangeForm {
        form: RefId::from_bytes([0x40, 0, 0x20]),
        change_flags: 0,
        type_field,
        version: 1,
        uncompressed_len: 0,
        data: vec![0xDE, 0xAD, 0xBE, 0xEF],
    });
    save
}

fn game_with_skyrim() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn opaque_projectile_bytes_are_refused(type_field: u8, expected: &str) {
    let directory = Directory::new();
    let store = directory.store();
    let save = recoverable_save(type_field);
    let before = save.write().unwrap();
    let mut game = game_with_skyrim();
    let snapshot = game.save_state().unwrap();

    let error =
        soe_godot_host::save_recovery::accept_vanilla_recovery_session(&mut game, &save, &store)
            .expect_err("opaque projectile payload must not be silently dropped");

    assert!(error.to_string().contains(expected), "{error}");
    assert_eq!(game.save_state().unwrap(), snapshot);
    assert_eq!(save.write().unwrap(), before);
    assert_eq!(
        fs::read_dir(directory.0.join("recovery")).unwrap().count(),
        0
    );
}

#[test]
fn sav_19_opaque_grenade_projectile_payload_is_not_silently_discarded() {
    // UESP/FallrimTools type 3 is PGRE; xEdit declares no decoded PGRE payload.
    opaque_projectile_bytes_are_refused(3, "PGRE");
}

#[test]
fn sav_19_opaque_hazard_payload_is_not_silently_discarded() {
    // UESP/FallrimTools type 40 is PHZD; xEdit declares no decoded PHZD payload.
    opaque_projectile_bytes_are_refused(40, "PHZD");
}

#[test]
fn sav_19_opaque_arrow_payload_is_not_silently_discarded() {
    // UESP/FallrimTools type 47 is PARW; xEdit declares no decoded PARW payload.
    opaque_projectile_bytes_are_refused(47, "PARW");
}
