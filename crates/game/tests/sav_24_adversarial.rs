// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{Compression, Header, RefId, Save, WString};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"PFLA"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("SAV-24"),
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
    soe_save::GlobalData { kind: 1000, data: vec![] }.write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.change_forms.push(soe_save::ChangeForm {
        form: reference(0x400800),
        change_flags: u32::MAX,
        type_field: 5,
        version: 78,
        uncompressed_len: 0,
        data: vec![0x11, 0x80, 0xFF],
    });
    save
}

#[test]
fn sav_24_repeated_projectile_restore_is_rejected_without_losing_saved_bytes() {
    let mut world = World::new(order());
    let saved = save();
    assert_eq!(world.restore_vanilla_projectiles(&saved).unwrap(), 1);
    let before = world.restored_projectile_extra(id(0x800)).unwrap().clone();
    assert!(world.restore_vanilla_projectiles(&saved).is_err());
    assert_eq!(world.restored_projectile_extra(id(0x800)), Some(&before));

    let bytes = world.save_state();
    let mut loaded = World::new(order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.restored_projectile_extra(id(0x800)), Some(&before));
}
