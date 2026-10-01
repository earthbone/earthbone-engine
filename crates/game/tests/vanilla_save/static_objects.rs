// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-42 applies and preserves CONT, MISC and FURN changes.

use super::*;

const FORM: u32 = 1;
const VALUE: u32 = 1 << 1;
const FULL_NAME: u32 = 1 << 2;

fn object_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x800).sub(
        Tag(*b"CNTO"),
        [0x801_u32.to_le_bytes(), 3_i32.to_le_bytes()].concat(),
    ));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801).zsub(Tag(*b"FULL"), "Saved item name"));
    plugin.push_top(Record::new(Tag(*b"FURN"), 0x802).sub(Tag(*b"FNAM"), [0x02]));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x810).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x820));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, mask: u32) -> soe_save::ChangeForm {
    let change_flags = FORM | VALUE | FULL_NAME | (1 << 31);
    let mut data = mask.to_le_bytes().to_vec();
    data.extend_from_slice(&0xBEEF_u16.to_le_bytes());
    data.extend_from_slice(&[0xDE, 0xAD]);
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn fixture() -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = vec![
        change(0x800, 14, 0x20),
        change(0x801, 18, 0x40),
        change(0x802, 22, 0x80),
    ];
    saved
}

#[test]
fn sav_42_static_object_inversion_masks_reach_all_three_live_records() {
    let mut world = World::new(object_order());
    let original_flags: Vec<_> = [0x800, 0x801, 0x802]
        .into_iter()
        .map(|object| world.record(id(0, object)).unwrap().flags)
        .collect();

    assert_eq!(world.restore_vanilla_static_objects(&fixture()).unwrap(), 3);
    for (object, original, mask) in [
        (0x800, original_flags[0], 0x20),
        (0x801, original_flags[1], 0x40),
        (0x802, original_flags[2], 0x80),
    ] {
        assert_eq!(world.record(id(0, object)).unwrap().flags, original ^ mask);
    }
}

#[test]
fn sav_42_independent_record_queries_keep_results_after_engine_save_load() {
    let order = std::sync::Arc::new(object_order());
    let mut world = World::from_shared_order(order.clone());
    world.restore_vanilla_static_objects(&fixture()).unwrap();
    assert_eq!(
        world.name_of(id(0, 0x801)).as_deref(),
        Some("Saved item name")
    );
    assert_eq!(world.item_count(id(0, 0x810), id(0, 0x801)), 3);
    assert_eq!(world.tag_of(id(0, 0x802)), Some(Tag(*b"FURN")));

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(
        loaded.name_of(id(0, 0x801)).as_deref(),
        Some("Saved item name")
    );
    assert_eq!(loaded.item_count(id(0, 0x810), id(0, 0x801)), 3);
    assert_eq!(loaded.tag_of(id(0, 0x802)), Some(Tag(*b"FURN")));
    for (object, mask) in [(0x800, 0x20), (0x801, 0x40), (0x802, 0x80)] {
        assert_eq!(
            loaded.record(id(0, object)).unwrap().flags,
            world.record(id(0, object)).unwrap().flags
        );
        let extra = loaded.restored_static_object_extra(id(0, object)).unwrap();
        assert_eq!(extra.form_flags, Some((mask, 0xBEEF)));
        assert!(extra.value_changed);
        assert!(extra.full_name_changed);
        assert_eq!(extra.opaque_suffix, [0xDE, 0xAD]);
    }
}

#[test]
fn sav_42_late_invalid_target_leaves_every_static_object_unchanged() {
    let mut world = World::new(object_order());
    let before = world.save_state();
    let mut saved = fixture();
    saved.change_forms[2].form = reference(0x400999);

    assert!(matches!(
        world.restore_vanilla_static_objects(&saved),
        Err(RestoreError::StaticObject { .. })
    ));
    assert_eq!(world.save_state(), before);
    assert_eq!(world.record(id(0, 0x800)).unwrap().flags, 0);
}

#[test]
fn sav_42_zero_byte_markers_do_not_consume_or_replace_record_data() {
    let mut world = World::new(object_order());
    world.restore_vanilla_static_objects(&fixture()).unwrap();

    assert_eq!(
        world.name_of(id(0, 0x801)).as_deref(),
        Some("Saved item name")
    );
    assert_eq!(
        world
            .record(id(0, 0x800))
            .unwrap()
            .all(Tag(*b"CNTO"))
            .count(),
        1
    );
    assert_eq!(
        world.record(id(0, 0x802)).unwrap().find(Tag(*b"FNAM")),
        Some(&[0x02][..])
    );
}
