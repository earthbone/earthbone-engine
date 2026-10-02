// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ai, package, world::PackRecordExtra, World};
use soe_save::{ChangeForm, Compression, GlobalData, Header, RefId, Save, WString};

const PACK_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x840,
};
const NAVM_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x841,
};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("SAV-48 adversarial"),
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

fn world_and_save() -> (World, Save) {
    let mut plugin_file = Plugin::new(FileHeader::default());
    plugin_file.push_top(package::build_record(
        PACK_ID.object,
        "SavedPack",
        0,
        package::Schedule::default(),
    ));
    plugin_file.push_top(
        Record::new(Tag(*b"NAVM"), NAVM_ID.object)
            .zsub(Tag(*b"EDID"), "SavedNavmesh")
            .zsub(Tag(*b"FULL"), "Original navmesh"),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin_file).unwrap();
    let mut saved = save();
    saved.change_forms = vec![
        ChangeForm {
            form: reference(0x400840),
            change_flags: (1 << 30) | (1 << 31),
            type_field: 32,
            version: 78,
            uncompressed_len: 0,
            data: vec![],
        },
        ChangeForm {
            form: reference(0x400841),
            change_flags: (1 << 1) | (1 << 2),
            type_field: 33,
            version: 78,
            uncompressed_len: 0,
            data: vec![],
        },
    ];
    (World::new(order), saved)
}

#[test]
fn duplicate_pack_change_rejects_the_whole_restore_batch() {
    let (mut world, mut saved) = world_and_save();
    saved.change_forms.push(saved.change_forms[0].clone());

    assert!(world.restore_vanilla_pack_navmesh(&saved).is_err());
    assert!(world.restored_pack_extra(PACK_ID).is_none());
    assert!(world.restored_navmesh_extra(NAVM_ID).is_none());
    assert_eq!(world.name_of(NAVM_ID).as_deref(), Some("Original navmesh"));
}

#[test]
fn type_mismatch_does_not_install_a_partial_record_override() {
    let (mut world, mut saved) = world_and_save();
    saved.change_forms[1].type_field = 32;

    assert!(world.restore_vanilla_pack_navmesh(&saved).is_err());
    assert!(world.restored_pack_extra(PACK_ID).is_none());
    assert!(world.restored_navmesh_extra(NAVM_ID).is_none());
    assert_eq!(world.name_of(NAVM_ID).as_deref(), Some("Original navmesh"));
}

#[test]
fn runtime_package_and_navmesh_queries_keep_markers_after_engine_roundtrip() {
    let (mut world, saved) = world_and_save();
    assert_eq!(world.restore_vanilla_pack_navmesh(&saved).unwrap(), 2);

    let runtime_package = ai::package(&world, PACK_ID).unwrap();
    assert!(runtime_package.saved_waiting);
    assert!(runtime_package.saved_never_run);
    assert_eq!(world.name_of(NAVM_ID).as_deref(), Some("Original navmesh"));

    let bytes = world.save_state();
    let (mut loaded, _) = world_and_save();
    loaded.load_state(&bytes).unwrap();

    let restored = ai::package(&loaded, PACK_ID).unwrap();
    assert!(restored.saved_waiting);
    assert!(restored.saved_never_run);
    assert_eq!(
        loaded.restored_pack_extra(PACK_ID),
        Some(PackRecordExtra {
            form_flags: None,
            waiting: true,
            never_run: true,
        })
    );
    assert!(
        loaded
            .restored_navmesh_extra(NAVM_ID)
            .unwrap()
            .value_changed
    );
    assert!(
        loaded
            .restored_navmesh_extra(NAVM_ID)
            .unwrap()
            .full_name_changed
    );
    assert_eq!(loaded.name_of(NAVM_ID).as_deref(), Some("Original navmesh"));
}
