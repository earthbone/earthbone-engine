// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-44 applies APPA and IDLM changes to live records.

use super::*;

fn base_object_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"APPA"), 0x800)
            .with_flags(0x10)
            .sub(Tag(*b"EDID"), b"SavedApparatus\0".to_vec())
            .sub(Tag(*b"FULL"), b"Saved apparatus\0".to_vec()),
    );
    plugin.push_top(
        Record::new(Tag(*b"IDLM"), 0x801)
            .with_flags(0x40)
            .sub(Tag(*b"EDID"), b"SavedIdleMarker\0".to_vec())
            .sub(Tag(*b"FULL"), b"Saved idle marker\0".to_vec()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn base_change(object: u32, kind: u8, mask: u32, unknown: u16) -> soe_save::ChangeForm {
    let mut data = mask.to_le_bytes().to_vec();
    data.extend(unknown.to_le_bytes());
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: 1 | 2 | 4,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn base_object_save(changes: Vec<soe_save::ChangeForm>) -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = changes;
    saved
}

#[test]
fn sav_44_appa_idlm_restore_is_atomic_when_a_later_change_is_malformed() {
    let order = Arc::new(base_object_order());
    let mut world = World::from_shared_order(order);
    let mut invalid_idle = base_change(0x801, 27, 0x800, 0x1234);
    invalid_idle.data.pop();
    let saved = base_object_save(vec![base_change(0x800, 19, 0x100, 0x5678), invalid_idle]);

    assert!(world.restore_vanilla_base_objects(&saved).is_err());
    assert_eq!(world.record(id(0, 0x800)).unwrap().flags, 0x10);
    assert!(world.restored_base_object_extra(id(0, 0x800)).is_none());
}

#[test]
fn sav_44_restored_appa_idlm_records_and_queries_survive_engine_save_load() {
    let order = Arc::new(base_object_order());
    let mut world = World::from_shared_order(order.clone());
    let saved = base_object_save(vec![
        base_change(0x800, 19, 0x100, 0x5678),
        base_change(0x801, 27, 0x400, 0x1234),
    ]);
    assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 2);

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();

    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(loaded.record(id(0, 0x800)).unwrap().flags, 0x110);
    assert_eq!(loaded.record(id(0, 0x801)).unwrap().flags, 0x440);
    assert_eq!(loaded.tag_of(id(0, 0x800)), Some(Tag(*b"APPA")));
    assert_eq!(loaded.tag_of(id(0, 0x801)), Some(Tag(*b"IDLM")));
    assert_eq!(
        loaded.name_of(id(0, 0x800)).as_deref(),
        Some("Saved apparatus")
    );
    assert_eq!(
        loaded.name_of(id(0, 0x801)).as_deref(),
        Some("Saved idle marker")
    );
    assert_eq!(
        loaded
            .restored_base_object_extra(id(0, 0x800))
            .unwrap()
            .form_flags,
        Some((0x100, 0x5678))
    );
    assert_eq!(
        loaded
            .restored_base_object_extra(id(0, 0x801))
            .unwrap()
            .change_flags,
        7
    );
}
