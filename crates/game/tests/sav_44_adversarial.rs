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

fn fixture(changes: Vec<ChangeForm>) -> Save {
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
    save.change_forms = changes;
    save
}

fn change(object: u32, kind: u8, mask: u32) -> ChangeForm {
    let mut data = mask.to_le_bytes().to_vec();
    data.extend_from_slice(&0xABCD_u16.to_le_bytes());
    ChangeForm {
        form: RefId::from_bytes([0x40, (object >> 8) as u8, object as u8]),
        change_flags: 7,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_44_masks_apply_once_and_repeated_restore_cannot_toggle_live_flags_back() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"APPA"), 0x800)
            .with_flags(0x10)
            .zsub(Tag(*b"FULL"), "Tool"),
    );
    plugin.push_top(
        Record::new(Tag(*b"IDLM"), 0x801)
            .with_flags(0x40)
            .zsub(Tag(*b"FULL"), "Marker"),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let shared = Arc::new(order);
    let mut world = World::from_shared_order(shared);
    let saved = fixture(vec![change(0x800, 19, 0x20), change(0x801, 27, 0x80)]);
    assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 2);
    assert_eq!(world.record(id(0x800)).unwrap().flags, 0x30);
    assert_eq!(world.record(id(0x801)).unwrap().flags, 0xC0);
    let after_first = world.save_state();
    assert!(world.restore_vanilla_base_objects(&saved).is_err());
    assert_eq!(world.save_state(), after_first);
    assert_eq!(world.name_of(id(0x800)).as_deref(), Some("Tool"));
}
