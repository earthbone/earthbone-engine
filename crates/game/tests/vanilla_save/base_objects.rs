// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const WOOP_OBJECT: u32 = 0x800;
const MGEF_OBJECT: u32 = 0x801;

fn base_object_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"WOOP"), WOOP_OBJECT)
            .zsub(Tag(*b"EDID"), "VoiceWord")
            .zsub(Tag(*b"FULL"), "Original word"),
    );
    let mut data = vec![0; 152];
    data[64..68].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), MGEF_OBJECT)
            .zsub(Tag(*b"EDID"), "Effect")
            .zsub(Tag(*b"FULL"), "Original effect")
            .sub(Tag(*b"DATA"), data),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, flags: u32) -> soe_save::ChangeForm {
    let mut data = Vec::new();
    if flags & 1 != 0 {
        data.extend_from_slice(&0xA5A5_5A5A_u32.to_le_bytes());
        data.extend_from_slice(&0xBEEF_u16.to_le_bytes());
    }
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn fixture(changes: Vec<soe_save::ChangeForm>) -> Save {
    let mut save = save();
    save.plugins.truncate(1);
    save.light_plugins.clear();
    save.change_forms = changes;
    save
}

#[test]
fn sav_50_woop_and_mgef_markers_reach_runtime_queries_after_engine_reload() {
    let order = Arc::new(base_object_order());
    let saved = fixture(vec![
        change(WOOP_OBJECT, 34, 0x87),
        change(MGEF_OBJECT, 35, 0x05),
    ]);
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 2);

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(
        loaded.name_of(id(0, WOOP_OBJECT)).as_deref(),
        Some("Original word")
    );
    assert_eq!(
        loaded.name_of(id(0, MGEF_OBJECT)).as_deref(),
        Some("Original effect")
    );
    assert_eq!(
        loaded
            .magic_effect(id(0, MGEF_OBJECT))
            .unwrap()
            .data
            .archetype,
        3
    );
    assert_eq!(
        loaded.restored_base_object_extra(id(0, WOOP_OBJECT)),
        Some(&soe_game::world::BaseObjectRecordExtra {
            form_flags: Some((0xA5A5_5A5A, 0xBEEF)),
            value_changed: true,
            full_name_changed: true,
            unnamed_flags: 0x80,
        })
    );
    assert_eq!(
        loaded.restored_base_object_extra(id(0, MGEF_OBJECT)),
        Some(&soe_game::world::BaseObjectRecordExtra {
            form_flags: Some((0xA5A5_5A5A, 0xBEEF)),
            value_changed: false,
            full_name_changed: true,
            unnamed_flags: 0,
        })
    );
}

#[test]
fn sav_50_late_invalid_change_leaves_records_and_metadata_unchanged() {
    let order = Arc::new(base_object_order());
    let saved = fixture(vec![change(WOOP_OBJECT, 34, 1), change(0x802, 35, 1)]);
    let mut world = World::from_shared_order(order);
    let before = world.name_of(id(0, WOOP_OBJECT));
    assert!(world.restore_vanilla_base_objects(&saved).is_err());
    assert_eq!(world.name_of(id(0, WOOP_OBJECT)), before);
    assert_eq!(world.restored_base_object_extra(id(0, WOOP_OBJECT)), None);
}
