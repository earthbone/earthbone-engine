// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn save_with(change_forms: Vec<ChangeForm>) -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Tester"),
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
    let mut bytes = Vec::new();
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
        table_3_offset: at,
        change_forms_offset: at,
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
    save.change_forms = change_forms;
    save
}

fn change(object: u32, kind: u8, flags: u32) -> ChangeForm {
    let mut payload = Vec::new();
    if flags & 1 != 0 {
        payload.extend_from_slice(&0x20_u32.to_le_bytes());
        payload.extend_from_slice(&0_u16.to_le_bytes());
    }
    ChangeForm {
        form: RefId::from_bytes([0x40, (object >> 8) as u8, object as u8]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data: payload,
    }
}

#[test]
fn sav_40_form_flag_mask_changes_the_live_record_and_is_atomic() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 0x800).with_flags(0x08));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x801).with_flags(0x10));
    plugin.push_top(Record::new(Tag(*b"MSTT"), 0x802).with_flags(0x40));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let shared = Arc::new(order);
    let mut world = World::from_shared_order(shared);

    assert_eq!(
        world
            .restore_vanilla_base_objects(&save_with(vec![
                change(0x800, 17, 1),
                change(0x801, 20, 1),
                change(0x802, 21, 1),
            ]))
            .unwrap(),
        3
    );
    assert_eq!(world.record(id(0x800)).unwrap().flags, 0x28);
    assert_eq!(world.record(id(0x801)).unwrap().flags, 0x30);
    assert_eq!(world.record(id(0x802)).unwrap().flags, 0x60);

    let before = world.save_state();
    let mut malformed = change(0x802, 21, 1);
    malformed.data.truncate(2);
    let saved = save_with(vec![change(0x801, 20, 1), malformed]);
    assert!(world.restore_vanilla_base_objects(&saved).is_err());
    assert_eq!(world.save_state(), before);
}
