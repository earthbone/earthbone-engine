// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;
use soe_save::{Compression, GlobalData, Header, RefId, Save, WString};
use std::sync::Arc;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn empty_save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Breaker"),
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

#[test]
fn sav_36_shared_form_flags_are_mask_inverts_for_book_note_and_key() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (tag, object, flags) in [
        (*b"BOOK", 0x810, 0xA5A5_0001),
        (*b"NOTE", 0x811, 0x5A5A_0002),
        (*b"KEYM", 0x812, 0xF00F_0003),
    ] {
        plugin.push_top(
            Record::new(Tag(tag), object)
                .with_flags(flags)
                .zsub(Tag(*b"EDID"), &format!("Breaker{object:03}")),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut saved = empty_save();
    saved.plugins = [WString::from_text("Skyrim.esm")].to_vec();
    saved.change_forms = vec![
        soe_save::ChangeForm {
            form: reference(0x400810),
            change_flags: 1 | 2 | 4 | (1 << 5) | (1 << 6),
            type_field: 13,
            version: 78,
            uncompressed_len: 0,
            data: [
                0x05_u32.to_le_bytes().as_slice(),
                0_u16.to_le_bytes().as_slice(),
            ]
            .concat(),
        },
        soe_save::ChangeForm {
            form: reference(0x400811),
            change_flags: 1 | (1 << 31),
            type_field: 28,
            version: 78,
            uncompressed_len: 0,
            data: [
                0x0A_u32.to_le_bytes().as_slice(),
                0_u16.to_le_bytes().as_slice(),
            ]
            .concat(),
        },
        soe_save::ChangeForm {
            form: reference(0x400812),
            change_flags: 1 | 2 | 4,
            type_field: 25,
            version: 78,
            uncompressed_len: 0,
            data: [
                0x0F_u32.to_le_bytes().as_slice(),
                0_u16.to_le_bytes().as_slice(),
            ]
            .concat(),
        },
    ];
    let order = Arc::new(order);
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.restore_vanilla_book_note_keys(&saved).unwrap(), 3);
    for (object, expected) in [
        (0x810, 0xA5A5_0004),
        (0x811, 0x5A5A_0008),
        (0x812, 0xF00F_000C),
    ] {
        assert_eq!(world.record(id(object)).unwrap().flags, expected);
    }
    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    for (object, expected) in [
        (0x810, 0xA5A5_0004),
        (0x811, 0x5A5A_0008),
        (0x812, 0xF00F_000C),
    ] {
        assert_eq!(loaded.record(id(object)).unwrap().flags, expected);
    }
}
