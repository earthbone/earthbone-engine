// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-24 saved projectile envelopes reach runtime queries and engine saves.

use super::*;

fn projectile_fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, tag) in [(0x800, *b"PFLA"), (0x801, *b"PHZD"), (0x802, *b"PBAR")] {
        plugin
            .push_top(Record::new(Tag(tag), object).sub(Tag(*b"DATA"), [1_u8, 2, 3, object as u8]));
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn projectile_save() -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = [
        (0x800, 5, 0x8000_0001, vec![0x11, 0x12]),
        (0x801, 40, 0x4000_0002, vec![0x21, 0x22, 0x23]),
        (0x802, 41, 0x2000_0004, vec![0x31]),
    ]
    .into_iter()
    .map(|(object, kind, change_flags, data)| soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    })
    .collect();
    saved
}

#[test]
fn sav_24_projectile_change_envelopes_reach_live_queries_and_engine_reload() {
    let order = Arc::new(projectile_fixture_order());
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(
        world
            .restore_vanilla_projectiles(&projectile_save())
            .unwrap(),
        3
    );
    for (object, flags, payload) in [
        (0x800, 0x8000_0001, [0x11, 0x12].as_slice()),
        (0x801, 0x4000_0002, [0x21, 0x22, 0x23].as_slice()),
        (0x802, 0x2000_0004, [0x31].as_slice()),
    ] {
        let id = id(0, object);
        let extra = world.restored_projectile_extra(id).unwrap();
        assert_eq!(extra.flags, flags);
        assert_eq!(extra.payload, payload);
        assert_eq!(
            world.record(id).unwrap().find(Tag(*b"DATA")),
            Some(&[1, 2, 3, object as u8][..])
        );
    }

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    for object in 0x800..=0x802 {
        assert_eq!(
            loaded.restored_projectile_extra(id(0, object)),
            world.restored_projectile_extra(id(0, object))
        );
    }
}

#[test]
fn sav_24_bad_late_projectile_change_leaves_all_live_forms_unchanged() {
    let mut world = World::new(projectile_fixture_order());
    let before = world.save_state();
    let mut saved = projectile_save();
    saved.change_forms[2].form = reference(0x400999);
    assert!(world.restore_vanilla_projectiles(&saved).is_err());
    assert_eq!(world.save_state(), before);
    for object in 0x800..=0x802 {
        assert!(world.restored_projectile_extra(id(0, object)).is_none());
    }
}
