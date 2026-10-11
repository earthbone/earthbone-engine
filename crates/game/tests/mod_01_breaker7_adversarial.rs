// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_save::reference::PlayerLocation;
use soe_save::{
    Compression, FileLocationTable, GlobalData, GlobalVariables, Header, RefId, Save, WString,
};
use std::{fs, path::PathBuf};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x806;
const SPELL: u32 = 0x801;
const ACTORS: [u32; 3] = [0x812, 0x813, 0x814];
const PLAYER_ID: u64 = 0x2A73_F01A;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

struct Fixtures(PathBuf);

impl Fixtures {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("mod01-breaker7-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("Seed_DISTR.ini"),
            "Spell = SeedSpell|SeedBase|NONE|NONE|NONE|1|50!\n",
        )
        .unwrap();
        Self(directory)
    }

    fn save(&self) -> PathBuf {
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
            data: vec![0; 64],
        }];
        save.change_forms = vec![soe_save::ChangeForm {
            form: RefId::from_bytes([0x40, 0, 0x14]),
            change_flags: 0,
            type_field: 1,
            version: 0,
            uncompressed_len: 0,
            data: vec![0; 8],
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
        let table_3_offset = change_forms_offset
            + save
                .change_forms
                .iter()
                .map(|form| 3 + 4 + 1 + 1 + 1 + 1 + form.data.len())
                .sum::<usize>();
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
        save.location_table.change_form_count = save.change_forms.len() as u32;
        let path = self
            .0
            .join("Quicksave0_2A73F01A_0_6E656C736F6E_Tamriel_000002_20220918174138_10_1.ess");
        fs::write(&path, save.write().unwrap()).unwrap();
        path
    }

    fn game(&self, save: &std::path::Path) -> Game {
        let mut plugin = Plugin::new(FileHeader {
            flags: flags::MASTER,
            ..Default::default()
        });
        plugin.push_top(Record::new(T(b"SPEL"), SPELL).zsub(T(b"EDID"), "SeedSpell"));
        plugin.push_top(npc::build_record(
            BASE,
            "SeedBase",
            "Seed Base",
            npc::Config::default(),
            npc::Stats::default(),
        ));
        plugin.push_top(Record::new(T(b"NPC_"), 7).zsub(T(b"EDID"), "Player"));
        plugin.push_top(
            Record::new(T(b"ACHR"), 0x14)
                .sub(T(b"NAME"), 7_u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
        for actor in ACTORS {
            plugin.push_top(
                Record::new(T(b"ACHR"), actor)
                    .sub(T(b"NAME"), BASE.to_le_bytes())
                    .sub(T(b"DATA"), vec![0; 24]),
            );
        }
        let mut order = LoadOrder::new();
        order
            .add("Skyrim.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
            .unwrap();
        let mut game = Game::new(order);
        game.world_mut().set_player_character_id(0x1234_5678);
        game.restore_vanilla_save_file(save).unwrap();
        game.apply_mod_data(std::slice::from_ref(&self.0));
        game
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mod_01_restored_save_id_drives_distinct_repeatable_spid_actor_results() {
    let fixtures = Fixtures::new();
    let save = fixtures.save();
    let mut first = fixtures.game(&save);
    assert_eq!(first.world().player_character_id(), PLAYER_ID);
    let results: Vec<_> = ACTORS
        .iter()
        .map(|actor| {
            first
                .world_mut()
                .actor(id(*actor))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    assert_eq!(results, [false, false, true]);

    let mut second = fixtures.game(&save);
    let reversed: Vec<_> = ACTORS
        .iter()
        .rev()
        .map(|actor| {
            second
                .world_mut()
                .actor(id(*actor))
                .unwrap()
                .spells
                .contains(&id(SPELL))
        })
        .collect();
    assert_eq!(reversed, [true, false, false]);
}
