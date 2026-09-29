// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
};

const WOOP: u32 = 0x800;
const MGEF: u32 = 0x801;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(valid_mgef: bool) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"WOOP"), WOOP)
            .with_flags(0x10)
            .zsub(Tag(*b"EDID"), "VoiceWord")
            .zsub(Tag(*b"FULL"), "Original word"),
    );
    let mut data = vec![0; if valid_mgef { 152 } else { 4 }];
    if valid_mgef {
        data[64..68].copy_from_slice(&3_u32.to_le_bytes());
    }
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), MGEF)
            .with_flags(0x05)
            .zsub(Tag(*b"EDID"), "Effect")
            .zsub(Tag(*b"FULL"), "Original effect")
            .sub(Tag(*b"DATA"), data),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, (object >> 8) as u8, object as u8]),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn form_flags(mask: u32, unknown: u16) -> Vec<u8> {
    [mask.to_le_bytes().as_slice(), unknown.to_le_bytes().as_slice()].concat()
}

fn save(changes: Vec<ChangeForm>) -> Save {
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

#[test]
fn sav_50_all_defined_flags_and_high_unnamed_bit_roundtrip_without_rewriting_records() {
    for kind_object in [(34, WOOP), (35, MGEF)] {
        for flags in (0..8).chain([1 << 31]) {
            let mut data = Vec::new();
            if flags & 1 != 0 {
                data.extend_from_slice(&0xA5A5_5A5A_u32.to_le_bytes());
                data.extend_from_slice(&0xBEEF_u16.to_le_bytes());
            }
            let order = Arc::new(order(true));
            let mut world = World::from_shared_order(order.clone());
            let saved = save(vec![change(kind_object.1, kind_object.0, flags, data)]);
            assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 1);
            let authored_flags = if kind_object.1 == WOOP { 0x10 } else { 0x05 };
            assert_eq!(world.record(id(kind_object.1)).unwrap().flags, authored_flags);
            let bytes = world.save_state();
            let mut loaded = World::from_shared_order(order);
            loaded.load_state(&bytes).unwrap();
            let extra = loaded.restored_base_object_extra(id(kind_object.1)).unwrap();
            assert_eq!(extra.change_flags, flags);
            assert_eq!(extra.form_flags.is_some(), flags & 1 != 0);
            assert_eq!(extra.value_changed, flags & 2 != 0);
            assert_eq!(extra.full_name_changed, flags & 4 != 0);
            assert_eq!(extra.unnamed_flags, flags & !7);
        }
    }
}

#[test]
fn sav_50_form_flag_mask_inverts_authored_flags_and_survives_engine_reload() {
    let order = Arc::new(order(true));
    let mut world = World::from_shared_order(order.clone());
    let saved = save(vec![
        change(WOOP, 34, 1, form_flags(0x03, 0xBEEF)),
        change(MGEF, 35, 1, form_flags(0x03, 0xCAFE)),
    ]);
    assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 2);
    assert_eq!(world.record(id(WOOP)).unwrap().flags, 0x10 ^ 0x03);
    assert_eq!(world.record(id(MGEF)).unwrap().flags, 0x05 ^ 0x03);

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.record(id(WOOP)).unwrap().flags, 0x10 ^ 0x03);
    assert_eq!(loaded.record(id(MGEF)).unwrap().flags, 0x05 ^ 0x03);
}

#[test]
fn sav_50_truncated_or_extra_payload_rejects_entire_restore() {
    let malformed = [
        change(WOOP, 34, 1, vec![1; 5]),
        change(MGEF, 35, 1, vec![1; 7]),
        change(WOOP, 34, 0, vec![1]),
        change(MGEF, 35, 0, vec![1]),
    ];
    for bad_change in malformed {
        let mut world = World::new(order(true));
        let before = world.save_state();
        assert!(world
            .restore_vanilla_base_objects(&save(vec![
                change(WOOP, 34, 1, vec![0; 6]),
                bad_change,
            ]))
            .is_err());
        assert_eq!(world.save_state(), before);
        assert_eq!(world.restored_base_object_extra(id(WOOP)), None);
        assert_eq!(world.restored_base_object_extra(id(MGEF)), None);
    }
}

#[test]
fn sav_50_invalid_mgef_record_rejects_prior_woop_atomically() {
    let mut world = World::new(order(false));
    let before = world.save_state();
    assert!(world
        .restore_vanilla_base_objects(&save(vec![
            change(WOOP, 34, 0, vec![]),
            change(MGEF, 35, 0, vec![]),
        ]))
        .is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(world.restored_base_object_extra(id(WOOP)), None);
}

#[test]
fn sav_50_duplicate_and_repeated_restore_reject_without_mutation() {
    let mut world = World::new(order(true));
    let one = save(vec![change(WOOP, 34, 0, vec![])]);
    assert_eq!(world.restore_vanilla_base_objects(&one).unwrap(), 1);
    let after_first = world.save_state();
    assert!(world
        .restore_vanilla_base_objects(&save(vec![
            change(MGEF, 35, 0, vec![]),
            change(WOOP, 34, 2, vec![]),
        ]))
        .is_err());
    assert_eq!(world.save_state(), after_first);
    assert!(world
        .restore_vanilla_base_objects(&one)
        .is_err());
    assert_eq!(world.save_state(), after_first);
}
