// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{
    reference::PlayerLocation, Compression, GlobalData, Header, RefId, Save, WString,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800).sub(Tag(*b"DATA"), [0, 0]));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x14).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x810));
    plugin.push_top(Record::new(Tag(*b"PMIS"), 0x900).sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn saved() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("SAV-01"),
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
    save.table_1.push(GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF000800,
            world: reference(0),
            coordinates: [0; 2],
            cell_or_world: reference(0x400800),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    let mut player = reference(0x400800).raw().to_be_bytes()[1..].to_vec();
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0] {
        player.extend(value.to_le_bytes());
    }
    player.extend([0; 8]);
    let mut projectile = reference(0x400800).raw().to_be_bytes()[1..].to_vec();
    for value in [4.0_f32, 5.0, 6.0, 0.0, 0.5, 0.0] {
        projectile.extend(value.to_le_bytes());
    }
    save.change_forms = vec![
        soe_save::ChangeForm {
            form: reference(0x400014),
            change_flags: 1 << 1,
            type_field: 1,
            version: 0,
            uncompressed_len: 0,
            data: player,
        },
        soe_save::ChangeForm {
            form: reference(0x400900),
            change_flags: 1 << 1,
            type_field: 2,
            version: 0,
            uncompressed_len: 0,
            data: projectile,
        },
    ];
    save
}

#[test]
fn sav_01_pmis_saved_move_resolves_its_record_type_and_survives_engine_reload() {
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&saved()).unwrap();
    assert_eq!(world.reference_ref(id(0x900)).unwrap().position, [4.0, 5.0, 6.0]);
    assert_eq!(world.reference(id(0x900)).unwrap().base, Some(id(0x820)));
    let bytes = world.save_state();
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.reference_ref(id(0x900)).unwrap().position, [4.0, 5.0, 6.0]);
}
