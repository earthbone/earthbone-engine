// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{Compression, Header, RefId, Save, WString};

const CLASS: u32 = 0x800;
const FACT: u32 = 0x801;
const RELA: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn save() -> Save {
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
    save
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"CLAS"), CLASS)
            .with_flags(0x100)
            .sub(Tag(*b"DATA"), [0; 16]),
    );
    plugin.push_top(
        Record::new(Tag(*b"FACT"), FACT)
            .with_flags(0x200)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"XNAM"), [0; 12]),
    );
    let mut relationship = vec![0; 16];
    relationship[0..4].copy_from_slice(&CLASS.to_le_bytes());
    relationship[4..8].copy_from_slice(&FACT.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"RELA"), RELA)
            .with_flags(0x400)
            .sub(Tag(*b"DATA"), relationship),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, flags: u32, data: Vec<u8>) -> soe_save::ChangeForm {
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
fn sav_46_shared_form_flags_xor_the_authored_flags_for_all_three_types() {
    let order = Arc::new(order());
    let mut saved = save();
    let mask = 0x10_u32;
    let mut form_flags = mask.to_le_bytes().to_vec();
    form_flags.extend_from_slice(&0x1234_u16.to_le_bytes());
    saved.change_forms = vec![
        change(CLASS, 30, 1, form_flags.clone()),
        change(FACT, 31, 1, form_flags.clone()),
        change(RELA, 39, 1, form_flags),
    ];

    let mut world = World::from_shared_order(order);
    world.restore_vanilla_record_changes(&saved).unwrap();
    assert_eq!(world.record(id(CLASS)).unwrap().flags, 0x100 ^ mask);
    assert_eq!(world.record(id(FACT)).unwrap().flags, 0x200 ^ mask);
    assert_eq!(world.record(id(RELA)).unwrap().flags, 0x400 ^ mask);
}

#[test]
fn sav_46_absent_form_flags_leave_all_authored_record_flags_unchanged() {
    let order = Arc::new(order());
    let mut saved = save();
    saved.change_forms = vec![
        change(CLASS, 30, 0, vec![]),
        change(FACT, 31, 0, vec![]),
        change(RELA, 39, 0, vec![]),
    ];
    let mut world = World::from_shared_order(order);
    world.restore_vanilla_record_changes(&saved).unwrap();
    assert_eq!(world.record(id(CLASS)).unwrap().flags, 0x100);
    assert_eq!(world.record(id(FACT)).unwrap().flags, 0x200);
    assert_eq!(world.record(id(RELA)).unwrap().flags, 0x400);
}

#[test]
fn sav_46_reapplying_the_same_change_fails_without_changing_saved_state() {
    let order = Arc::new(order());
    let mut saved = save();
    saved.change_forms = vec![change(FACT, 31, 2, 0x40_u32.to_le_bytes().to_vec())];
    let mut world = World::from_shared_order(order);
    world.restore_vanilla_record_changes(&saved).unwrap();
    let before = world.save_state();
    assert!(world.restore_vanilla_record_changes(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.faction(id(FACT)).unwrap().tracks_crime);
}

#[test]
fn sav_46_relationship_rank_above_source_range_is_rejected_atomically() {
    let order = Arc::new(order());
    let mut saved = save();
    saved
        .change_forms
        .push(change(RELA, 39, 2, 9_i32.to_le_bytes().to_vec()));
    let mut world = World::from_shared_order(order);
    let before = world.save_state();
    let authored_data = world
        .record(id(RELA))
        .unwrap()
        .find(Tag(*b"DATA"))
        .unwrap()
        .to_vec();

    assert!(world.restore_vanilla_record_changes(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(
        world.record(id(RELA)).unwrap().find(Tag(*b"DATA")),
        Some(authored_data.as_slice())
    );
}

#[test]
fn sav_46_relationship_rank_below_signed_range_is_rejected_atomically() {
    let order = Arc::new(order());
    let mut saved = save();
    saved
        .change_forms
        .push(change(RELA, 39, 2, (-5_i32).to_le_bytes().to_vec()));
    let mut world = World::from_shared_order(order);
    let before = world.save_state();

    assert!(world.restore_vanilla_record_changes(&saved).is_err());
    assert_eq!(world.save_state(), before);
}
