// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_save::reference::PlayerLocation;
use soe_save::{
    Compression, FileLocationTable, GlobalData, GlobalVariables, Header, RefId, Save, WString,
};
use std::{fs, path::PathBuf};

struct SaveFiles(PathBuf);

impl SaveFiles {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("mod01-save-seed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str) -> PathBuf {
        let header = Header {
            version: 12,
            save_number: 1,
            player_name: WString::from_text("Player"),
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
        let mut save = Save::parse(&bytes).unwrap();
        save.table_1 = vec![
            GlobalData {
                kind: 1,
                data: PlayerLocation {
                    next_object_id: 0xFF00_0001,
                    world: RefId::from_bytes([0; 3]),
                    coordinates: [0, 0],
                    cell_or_world: RefId::from_bytes([0; 3]),
                    position: [0.0; 3],
                    unknown: 0,
                }
                .data_bytes(),
            },
            GlobalData {
                kind: GlobalData::GLOBAL_VARIABLES,
                data: GlobalVariables { variables: vec![] }.write().unwrap(),
            },
        ];
        save.table_3 = vec![GlobalData {
            kind: GlobalData::PAPYRUS,
            data: vec![],
        }];
        let mut plugin_info = vec![save.plugins.len() as u8];
        for plugin in &save.plugins {
            plugin.write(&mut plugin_info);
        }
        plugin_info.extend_from_slice(&(save.light_plugins.len() as u16).to_le_bytes());
        for plugin in &save.light_plugins {
            plugin.write(&mut plugin_info);
        }
        let table_1_offset = save.header_len() + 1 + 4 + plugin_info.len() + 100;
        let table_2_offset = table_1_offset
            + save
                .table_1
                .iter()
                .map(|block| 8 + block.data.len())
                .sum::<usize>();
        let change_forms_offset = table_2_offset;
        let table_3_offset = change_forms_offset;
        let form_id_array_count_offset = table_3_offset
            + save
                .table_3
                .iter()
                .map(|block| 8 + block.data.len())
                .sum::<usize>();
        save.location_table.table_1_offset = table_1_offset as u32;
        save.location_table.table_2_offset = table_2_offset as u32;
        save.location_table.change_forms_offset = change_forms_offset as u32;
        save.location_table.table_3_offset = table_3_offset as u32;
        save.location_table.form_id_array_count_offset = form_id_array_count_offset as u32;
        save.location_table.unknown_table_3_offset = (form_id_array_count_offset + 8) as u32;
        save.location_table.table_1_count = 2;
        save.location_table.table_2_count = 0;
        save.location_table.table_3_count = 0;
        save.location_table.change_form_count = 0;
        let path = self.0.join(name);
        fs::write(path.as_path(), save.write().unwrap()).unwrap();
        path
    }
}

impl Drop for SaveFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mod_01_failed_filename_seeded_restore_keeps_host_id() {
    let files = SaveFiles::new();
    let standard =
        files.write("Quicksave0_2A73F01A_0_6E656C736F6E_Tamriel_000002_20220918174138_10_1.ess");
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    game.world_mut().set_player_character_id(0x1234_5678);

    assert!(game.restore_vanilla_save_file(&standard).is_err());
    assert_eq!(
        game.world().player_character_id(),
        0x1234_5678,
        "failed vanilla restoration must roll back the filename-derived SPID seed"
    );
}
