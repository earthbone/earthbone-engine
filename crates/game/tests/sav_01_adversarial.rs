// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::FileHeader};
use soe_game::World;
use soe_save::{Compression, GlobalData, Header, RefId, Save, WString, reference::PlayerLocation};

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
    for (tag, object) in [(*b"PMIS", 0x900), (*b"PGRE", 0x901), (*b"PBEA", 0x902)] {
        plugin.push_top(
            Record::new(Tag(tag), object)
                .sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes())
                .sub(Tag(*b"DATA"), [0_u8; 24]),
        );
    }
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
    soe_save::GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
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
    let mut changes = vec![soe_save::ChangeForm {
        form: reference(0x400014),
        change_flags: 1 << 1,
        type_field: 1,
        version: 0,
        uncompressed_len: 0,
        data: player,
    }];
    for (object, type_field, position) in [
        (0x900, 2, [4.0_f32, 5.0, 6.0]),
        (0x901, 3, [7.0_f32, 8.0, 9.0]),
        (0x902, 4, [10.0_f32, 11.0, 12.0]),
    ] {
        let mut data = reference(0x400800).raw().to_be_bytes()[1..].to_vec();
        for value in position.into_iter().chain([0.0, 0.5, 0.0]) {
            data.extend(value.to_le_bytes());
        }
        changes.push(soe_save::ChangeForm {
            form: reference(0x400000 | object),
            change_flags: 1 << 1,
            type_field,
            version: 0,
            uncompressed_len: 0,
            data,
        });
    }
    save.change_forms = changes;
    save
}

#[test]
fn sav_01_pmis_saved_move_resolves_its_record_type_and_survives_engine_reload() {
    let mut world = World::new(fixture_order());
    world
        .restore_vanilla_reference_placements(&saved())
        .unwrap();
    for (object, position) in [
        (0x900, [4.0, 5.0, 6.0]),
        (0x901, [7.0, 8.0, 9.0]),
        (0x902, [10.0, 11.0, 12.0]),
    ] {
        assert_eq!(world.reference_ref(id(object)).unwrap().position, position);
        assert_eq!(world.reference(id(object)).unwrap().base, Some(id(0x820)));
    }
    let bytes = world.save_state();
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    for (object, position) in [
        (0x900, [4.0, 5.0, 6.0]),
        (0x901, [7.0, 8.0, 9.0]),
        (0x902, [10.0, 11.0, 12.0]),
    ] {
        assert_eq!(loaded.reference_ref(id(object)).unwrap().position, position);
        assert_eq!(loaded.reference(id(object)).unwrap().base, Some(id(0x820)));
    }
}

#[test]
fn sav_01_late_projectile_failure_leaves_reference_restore_atomic() {
    let mut world = World::new(fixture_order());
    let before = world.save_state();
    let mut invalid = saved();
    invalid.change_forms[3].form = reference(0x400999);

    assert!(
        world
            .restore_vanilla_reference_placements(&invalid)
            .is_err()
    );
    assert_eq!(world.save_state(), before);
    for object in 0x900..=0x902 {
        assert!(world.reference_ref(id(object)).is_none());
    }
}
