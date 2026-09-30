// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{vanilla_save::RestoreError, Game};
use soe_save::{
    reference::{flags as change_flags, PlayerLocation}, ChangeForm, Compression, GlobalData,
    GlobalVariable, GlobalVariables, Header, RefId, Save, WString,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    plugin.push_top(
        Record::new(Tag(*b"GLOB"), 0x900)
            .sub(Tag(*b"FLTV"), 11.0_f32.to_le_bytes().to_vec()),
    );
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn save() -> Save {
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
    soe_save::FileLocationTable {
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

fn with_global_change() -> Save {
    let mut save = save();
    save.form_id_array = vec![0x900];
    save.table_1 = vec![
        GlobalData {
            kind: 1,
            data: PlayerLocation {
                next_object_id: 0xFF00_0001,
                world: reference(0),
                coordinates: [0; 2],
                cell_or_world: reference(0x40_0800),
                position: [1.0, 2.0, 3.0],
                unknown: 0,
            }
            .data_bytes(),
        },
        GlobalData {
            kind: GlobalData::GLOBAL_VARIABLES,
            data: GlobalVariables {
            variables: vec![GlobalVariable {
                form: reference(1),
                value: 42.5,
            }],
        }
        .write()
        .unwrap(),
        },
    ];
    let mut placement = vec![];
    reference(0x40_0800).write(&mut placement);
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.5, -0.0] {
        placement.extend_from_slice(&value.to_le_bytes());
    }
    placement.extend([0; 8]);
    save.change_forms = vec![ChangeForm {
        form: reference(0x40_0014),
        change_flags: change_flags::MOVE,
        type_field: 1,
        version: 0,
        uncompressed_len: 0,
        data: placement,
    }];
    save
}

fn assert_rollback(save: &Save, expected_error: impl FnOnce(&RestoreError) -> bool) {
    let mut game = game();
    game.world_mut().set_global(id(0x900), 7.0);
    game.vm.create_object("prior VM object");
    let before = game.save_state().unwrap();

    let error = game.restore_vanilla_world_and_vm(save).unwrap_err();
    assert!(expected_error(&error), "unexpected restore error: {error:?}");
    assert_eq!(game.world_mut().global_value(id(0x900)), 7.0);
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn sav_09_missing_final_papyrus_block_rolls_back_world_and_vm() {
    let save = with_global_change();
    assert_rollback(&save, |error| matches!(error, RestoreError::PapyrusBlock));
}

#[test]
fn sav_09_truncated_final_papyrus_block_rolls_back_world_and_vm() {
    let mut save = with_global_change();
    save.table_3 = vec![GlobalData {
        kind: GlobalData::PAPYRUS,
        data: vec![0xff],
    }];
    assert_rollback(&save, |error| matches!(error, RestoreError::Save(_)));
}

#[test]
fn sav_09_repeated_final_pass_failures_keep_the_original_snapshot() {
    let mut game = game();
    game.world_mut().set_global(id(0x900), 7.0);
    game.vm.create_object("prior VM object");
    let before = game.save_state().unwrap();

    for bytes in [vec![], vec![0xff], vec![2, 0, 0, 0]] {
        let mut save = with_global_change();
        save.table_3 = vec![GlobalData {
            kind: GlobalData::PAPYRUS,
            data: bytes,
        }];
        assert!(game.restore_vanilla_world_and_vm(&save).is_err());
        assert_eq!(game.world_mut().global_value(id(0x900)), 7.0);
        assert_eq!(game.save_state().unwrap(), before);
    }
}
