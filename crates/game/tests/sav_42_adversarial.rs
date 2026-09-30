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
    data.extend_from_slice(&0xBEEF_u16.to_le_bytes());
    data.extend_from_slice(&[0xDE, 0xAD]);
    ChangeForm {
        form: RefId::from_bytes([0x40, (object >> 8) as u8, object as u8]),
        change_flags: 0x8000_0007,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_42_each_record_type_xors_its_mask_and_keeps_unrelated_inventory_data() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x800).with_flags(0x11).sub(
        Tag(*b"CNTO"),
        [0x801_u32.to_le_bytes(), 4_i32.to_le_bytes()].concat(),
    ));
    plugin.push_top(
        Record::new(Tag(*b"MISC"), 0x801)
            .with_flags(0x02)
            .zsub(Tag(*b"FULL"), "Item"),
    );
    plugin.push_top(Record::new(Tag(*b"FURN"), 0x802).with_flags(0x44));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x810).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let shared = Arc::new(order);
    let mut world = World::from_shared_order(Arc::clone(&shared));
    assert_eq!(world.record(id(0x800)).unwrap().tag, Tag(*b"CONT"));
    assert_eq!(world.record(id(0x801)).unwrap().tag, Tag(*b"MISC"));
    assert_eq!(world.record(id(0x802)).unwrap().tag, Tag(*b"FURN"));
    let saved = fixture(vec![
        change(0x800, 14, 0x10),
        change(0x801, 18, 0x20),
        change(0x802, 22, 0x40),
    ]);
    assert_eq!(world.restore_vanilla_static_objects(&saved).unwrap(), 3);
    assert_eq!(world.record(id(0x800)).unwrap().flags, 0x01);
    assert_eq!(world.record(id(0x801)).unwrap().flags, 0x22);
    assert_eq!(world.record(id(0x802)).unwrap().flags, 0x04);
    assert_eq!(world.item_count(id(0x810), id(0x801)), 4);

    let saved_world = world.save_state();
    let mut restored = World::from_shared_order(shared);
    restored.load_state(&saved_world).unwrap();
    assert_eq!(restored.record(id(0x800)).unwrap().flags, 0x01);
    assert_eq!(restored.record(id(0x801)).unwrap().flags, 0x22);
    assert_eq!(restored.record(id(0x802)).unwrap().flags, 0x04);
    assert_eq!(restored.item_count(id(0x810), id(0x801)), 4);
    assert_eq!(restored.save_state(), saved_world);

    let before = world.save_state();
    let duplicate = fixture(vec![change(0x802, 22, 1), change(0x802, 22, 2)]);
    assert!(world.restore_vanilla_static_objects(&duplicate).is_err());
    assert_eq!(world.save_state(), before);
}
