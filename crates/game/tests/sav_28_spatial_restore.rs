// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{Compression, Header, RefId, Save, WString};
use std::sync::Arc;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn common_flags(flags: u32, unknown: u16) -> Vec<u8> {
    let mut bytes = flags.to_le_bytes().to_vec();
    bytes.extend(unknown.to_le_bytes());
    bytes
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("SAV-28"),
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
    let mut saved = Save::parse(&bytes).unwrap();
    saved.plugins = vec![WString::from_text("Skyrim.esm")];
    saved
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800).sub(Tag(*b"DATA"), [0, 0]));
    plugin.push_top(Record::new(Tag(*b"LCTN"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ECZN"), 0x802));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x810));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn spatial_change(object: u32, kind: u8, flags: u32, data: Vec<u8>) -> soe_save::ChangeForm {
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_28_spatial_record_changes_are_atomic_and_survive_engine_reload() {
    let shared = Arc::new(order());
    let mut world = World::from_shared_order(shared.clone());
    assert_eq!(world.cell_state(id(0x800)).unwrap().owner, None);

    let mut saved = save();
    saved.change_forms = vec![
        spatial_change(0x800, 6, 1 | 2 | 4 | 8, vec![]),
        spatial_change(0x801, 38, 1, common_flags(0x8765_4321, 0x1234)),
        spatial_change(0x802, 29, 1, common_flags(0x7654_3210, 0x5678)),
    ];
    let mut cell = 0x1234_5678_u32.to_le_bytes().to_vec();
    cell.extend(0x5678_u16.to_le_bytes());
    cell.extend(0x9ABC_u16.to_le_bytes());
    cell.extend(4_u16.to_le_bytes());
    cell.extend_from_slice(b"Hall");
    cell.extend(reference(0x400810).raw().to_be_bytes()[1..].iter().copied());
    saved.change_forms[0].data = cell;

    assert_eq!(world.restore_vanilla_spatial_records(&saved).unwrap(), 3);
    assert_eq!(world.record(id(0x800)).unwrap().flags, 0x1234_5678);
    assert_eq!(world.record(id(0x801)).unwrap().flags, 0x8765_4321);
    assert_eq!(world.record(id(0x802)).unwrap().flags, 0x7654_3210);
    assert_eq!(
        world.record(id(0x800)).unwrap().find(Tag(*b"DATA")),
        Some(&[0xBC, 0x9A][..])
    );
    assert_eq!(world.name_of(id(0x800)).as_deref(), Some("Hall"));
    assert_eq!(world.cell_state(id(0x800)).unwrap().owner, Some(id(0x810)));

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(shared);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.name_of(id(0x800)).as_deref(), Some("Hall"));
    assert_eq!(loaded.cell_state(id(0x800)).unwrap().owner, Some(id(0x810)));
    assert_eq!(loaded.record(id(0x801)).unwrap().flags, 0x8765_4321);
    assert_eq!(loaded.record(id(0x802)).unwrap().flags, 0x7654_3210);
    assert_eq!(loaded.restored_spatial_payload(id(0x801)), Some(&[][..]));
    assert_eq!(loaded.restored_spatial_payload(id(0x802)), Some(&[][..]));
}

#[test]
fn sav_28_bad_later_spatial_payload_leaves_earlier_records_unchanged() {
    let mut world = World::from_shared_order(Arc::new(order()));
    let mut saved = save();
    saved.change_forms = vec![
        spatial_change(0x800, 6, 2, 0x2222_u16.to_le_bytes().to_vec()),
        spatial_change(0x801, 38, 1 << 12, vec![]),
    ];
    assert!(world.restore_vanilla_spatial_records(&saved).is_err());
    assert_eq!(
        world.record(id(0x800)).unwrap().find(Tag(*b"DATA")),
        Some(&[0, 0][..])
    );
}
