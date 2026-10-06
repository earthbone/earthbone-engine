// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;
use soe_save::{Compression, GlobalData, Header, RefId, Save, WString};

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
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = [WString::from_text("Skyrim.esm")].to_vec();
    save.light_plugins.clear();
    save.form_id_array = vec![0x800];
    save
}

#[test]
fn sav_32_mask_invert_flags_toggle_only_the_saved_bits() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"TACT"), 0x800)
            .with_flags(0xA5A5_0001)
            .zsub(Tag(*b"EDID"), "TalkingDoor"),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut saved = empty_save();
    saved.change_forms.push(soe_save::ChangeForm {
        form: reference(0x400800),
        change_flags: 1,
        type_field: 11,
        version: 78,
        uncompressed_len: 0,
        data: [
            0x05_u32.to_le_bytes().as_slice(),
            0_u16.to_le_bytes().as_slice(),
        ]
        .concat(),
    });
    let mut world = World::new(order);
    assert_eq!(world.restore_vanilla_activators(&saved).unwrap(), 1);
    assert_eq!(world.record(id(0x800)).unwrap().flags, 0xA5A5_0004);
}
