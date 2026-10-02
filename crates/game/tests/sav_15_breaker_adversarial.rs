#![allow(clippy::unwrap_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::World;
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
    reference::{PlayerLocation, flags},
};

fn ref_id(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn fixture() -> Save {
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
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.table_1 = vec![GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF000804,
            world: ref_id(0),
            coordinates: [0; 2],
            cell_or_world: ref_id(0),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    }];
    let mut player_data = vec![];
    ref_id(0x400800).write(&mut player_data);
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0] {
        player_data.extend_from_slice(&value.to_le_bytes());
    }
    player_data.extend_from_slice(&0_u32.to_le_bytes());
    player_data.extend_from_slice(&0_u16.to_le_bytes());
    player_data.extend([0; 8]);
    save.change_forms = vec![ChangeForm {
        form: ref_id(0x400014),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 1,
        version: 79,
        uncompressed_len: 0,
        data: player_data,
    }];
    save
}

#[test]
fn sav_15_rejects_unobserved_change_form_version_before_world_mutation() {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut world = World::new(order);
    let before = world.state_hash();
    let error = world
        .restore_vanilla_reference_placements(&fixture())
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("only validated for save version 12 and form versions 78"),
        "{error}"
    );
    assert_eq!(world.state_hash(), before);
}
