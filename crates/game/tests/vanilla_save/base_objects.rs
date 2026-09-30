// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-40 applies saved light and static base-object metadata to live records.

use super::*;

fn base_object_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, tag, name) in [
        (0x800, *b"LIGH", "Saved Light"),
        (0x801, *b"STAT", "Saved Static"),
        (0x802, *b"MSTT", "Saved Marker"),
    ] {
        let editor_id = format!("Test{object:03X}");
        plugin.push_top(
            Record::new(Tag(tag), object)
                .zsub(Tag(*b"EDID"), &editor_id)
                .zsub(Tag(*b"FULL"), name),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn base_object_change(object: u32, kind: u8, flags: u32) -> soe_save::ChangeForm {
    let mut data = Vec::new();
    if flags & 1 != 0 {
        data.extend(0xA5A5_0001_u32.to_le_bytes());
        data.extend(0x7B7B_u16.to_le_bytes());
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

fn base_object_save(changes: Vec<soe_save::ChangeForm>) -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = changes;
    saved
}

#[test]
fn sav_40_ligh_stat_mstt_changes_reach_record_queries_and_survive_engine_save_load() {
    let order = Arc::new(base_object_order());
    let saved = base_object_save(vec![
        base_object_change(0x800, 17, 0b111),
        base_object_change(0x801, 20, 0b101),
        base_object_change(0x802, 21, 0b011),
    ]);
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 3);

    for (object, expected_name, expected_flags) in [
        (
            0x800,
            "Saved Light",
            (Some((0xA5A5_0001, 0x7B7B)), true, true),
        ),
        (
            0x801,
            "Saved Static",
            (Some((0xA5A5_0001, 0x7B7B)), false, true),
        ),
        (
            0x802,
            "Saved Marker",
            (Some((0xA5A5_0001, 0x7B7B)), true, false),
        ),
    ] {
        let id = id(0, object);
        assert_eq!(world.name_of(id).as_deref(), Some(expected_name));
        assert!(world.record(id).is_some());
        let extra = world.restored_base_object_extra(id).unwrap();
        assert_eq!(
            (
                extra.form_flags,
                extra.value_changed,
                extra.full_name_changed
            ),
            expected_flags
        );
    }

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    for (object, expected_name) in [
        (0x800, "Saved Light"),
        (0x801, "Saved Static"),
        (0x802, "Saved Marker"),
    ] {
        let id = id(0, object);
        assert_eq!(loaded.name_of(id).as_deref(), Some(expected_name));
        assert!(loaded.record(id).is_some());
        assert_eq!(
            loaded.restored_base_object_extra(id),
            world.restored_base_object_extra(id)
        );
    }
}

#[test]
fn sav_40_all_defined_flag_combinations_apply_for_each_base_object_type() {
    for kind in [17, 20, 21] {
        for flags in 0..8 {
            let mut plugin = Plugin::new(FileHeader::default());
            let object = 0x800;
            let tag = match kind {
                17 => *b"LIGH",
                20 => *b"STAT",
                _ => *b"MSTT",
            };
            plugin.push_top(Record::new(Tag(tag), object));
            let mut order = LoadOrder::new();
            order.add("Skyrim.esm", plugin).unwrap();
            let saved = base_object_save(vec![base_object_change(object, kind, flags)]);
            let mut world = World::new(order);
            assert_eq!(world.restore_vanilla_base_objects(&saved).unwrap(), 1);
            let expected = (flags & 1 != 0, flags & 2 != 0, flags & 4 != 0);
            let extra = world.restored_base_object_extra(id(0, object)).unwrap();
            assert_eq!(
                (
                    extra.form_flags.is_some(),
                    extra.value_changed,
                    extra.full_name_changed
                ),
                expected
            );
        }
    }
}

#[test]
fn sav_40_malformed_or_late_invalid_change_leaves_records_unchanged() {
    let order = base_object_order();
    let mut world = World::new(order);
    let before = world.save_state();
    let mut invalid = vec![
        base_object_change(0x801, 20, 0x8),
        base_object_change(0x801, 20, 1),
        base_object_change(0x801, 20, 0),
    ];
    invalid[1].data.clear();
    invalid[2].data.push(0);
    let mut late_invalid = vec![base_object_change(0x800, 17, 1)];
    late_invalid.push(base_object_change(0x801, 20, 0));
    late_invalid[1].change_flags = 0x8;
    for malformed in invalid {
        let saved = base_object_save(vec![base_object_change(0x800, 17, 1), malformed]);
        assert!(world.restore_vanilla_base_objects(&saved).is_err());
        assert_eq!(world.save_state(), before);
    }
    assert!(world
        .restore_vanilla_base_objects(&base_object_save(late_invalid))
        .is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(world.restored_base_object_extra(id(0, 0x800)), None);
    assert_eq!(world.name_of(id(0, 0x800)).as_deref(), Some("Saved Light"));
}

#[test]
fn sav_40_duplicate_change_is_rejected_without_partial_application() {
    let mut world = World::new(base_object_order());
    let before = world.save_state();
    let saved = base_object_save(vec![
        base_object_change(0x800, 17, 0b001),
        base_object_change(0x800, 17, 0b010),
    ]);
    assert!(world.restore_vanilla_base_objects(&saved).is_err());
    assert_eq!(world.save_state(), before);
}
