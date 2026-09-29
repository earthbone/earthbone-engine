// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-34 applies decoded item fields through the shared record override table.

use super::*;

fn item_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"ARMO"), 0x800)
            .zsub(Tag(*b"EDID"), "Sav34Armor")
            .zsub(Tag(*b"FULL"), "Armor")
            .sub(Tag(*b"BOD2"), 0x200_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0x801)
            .zsub(Tag(*b"EDID"), "Sav34Weapon")
            .zsub(Tag(*b"FULL"), "Weapon")
            .sub(Tag(*b"DATA"), [1, 0, 0, 0, 0, 0])
            .sub(Tag(*b"DNAM"), [10, 0, 0, 0, 0, 0, 0, 0]),
    );
    plugin.push_top(
        Record::new(Tag(*b"AMMO"), 0x802)
            .zsub(Tag(*b"EDID"), "Sav34Ammo")
            .zsub(Tag(*b"FULL"), "Ammo")
            .sub(Tag(*b"DATA"), [1, 0, 0, 0, 0, 0, 0, 0]),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn item_change(object: u32, kind: u8, flags: u32, tail: &[u8]) -> soe_save::ChangeForm {
    let mut data = Vec::new();
    if flags & 1 != 0 {
        data.extend([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    }
    data.extend_from_slice(tail);
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn saved_items() -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = vec![
        item_change(0x800, 12, 1 | 2 | 0x8000_0020, &[0xA5, 0x5A]),
        item_change(0x801, 23, 2 | 4, &[0xCC]),
        item_change(0x802, 24, 4 | 0x100, &[0xDD, 0xEE]),
    ];
    saved
}

#[test]
fn sav_34_item_changes_update_shared_records_and_survive_engine_save_load() {
    let order = Arc::new(item_order());
    let mut world = World::from_shared_order(order.clone());
    let names = [
        world.name_of(id(0, 0x800)).unwrap(),
        world.name_of(id(0, 0x801)).unwrap(),
        world.name_of(id(0, 0x802)).unwrap(),
    ];
    let records: Vec<_> = [0x800, 0x801, 0x802]
        .into_iter()
        .map(|object| world.record(id(0, object)).unwrap().clone())
        .collect();

    assert_eq!(world.restore_vanilla_items(&saved_items()).unwrap(), 3);
    assert_eq!(
        [0x800, 0x801, 0x802].map(|object| world.name_of(id(0, object)).unwrap()),
        names
    );
    let restored_records: Vec<_> = [0x800, 0x801, 0x802]
        .into_iter()
        .map(|object| world.record(id(0, object)).unwrap().clone())
        .collect();
    assert_eq!(restored_records, records);
    assert_eq!(
        world.restored_item_extra(id(0, 0x800)).unwrap(),
        &soe_game::world::ItemRecordExtra {
            form_flags: Some((0x7856_3412, 0xBC9A)),
            value_changed: true,
            full_name_changed: false,
            unnamed_flags: 0x8000_0020,
            trailing_data: vec![0xA5, 0x5A],
        }
    );
    assert_eq!(
        world
            .restored_item_extra(id(0, 0x801))
            .unwrap()
            .trailing_data,
        [0xCC]
    );

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(loaded.name_of(id(0, 0x801)).as_deref(), Some("Weapon"));
    assert_eq!(
        loaded.restored_item_extra(id(0, 0x802)).unwrap(),
        &soe_game::world::ItemRecordExtra {
            form_flags: None,
            value_changed: false,
            full_name_changed: true,
            unnamed_flags: 0x100,
            trailing_data: vec![0xDD, 0xEE],
        }
    );
}

#[test]
fn sav_34_late_invalid_item_change_preserves_all_records() {
    let mut world = World::new(item_order());
    let before = world.save_state();
    let mut saved = saved_items();
    saved.change_forms[2].form = reference(0x400999);

    assert!(world.restore_vanilla_items(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.restored_item_extra(id(0, 0x800)).is_none());
    assert_eq!(world.name_of(id(0, 0x800)).as_deref(), Some("Armor"));
}

#[test]
fn sav_34_unknown_engine_save_version_is_rejected_atomically() {
    let mut world = World::new(item_order());
    world.restore_vanilla_items(&saved_items()).unwrap();
    let before = world.save_state();
    let mut unknown = before.clone();
    unknown[4..6].copy_from_slice(&38_u16.to_le_bytes());

    assert!(world.load_state(&unknown).is_err());
    assert_eq!(world.save_state(), before);
}

#[test]
fn sav_34_installed_vanilla_item_changes_apply_to_all_three_record_types() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut order = LoadOrder::new();
    order
        .add(
            "Skyrim.esm",
            Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap(),
        )
        .unwrap();
    let selected: Vec<_> = [*b"ARMO", *b"WEAP", *b"AMMO"]
        .map(|tag| {
            order
                .winners()
                .find(|(form, rref)| {
                    let record = order.record(*rref);
                    form.file == 0 && record.tag == Tag(tag) && !record.is_deleted()
                })
                .map(|(form, _)| form)
                .unwrap()
        })
        .into_iter()
        .collect();
    let kinds = [12, 23, 24];
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = selected
        .iter()
        .zip(kinds)
        .map(|(form, kind)| item_change(form.object, kind, 0, &[]))
        .collect();
    let mut world = World::new(order);

    assert_eq!(world.restore_vanilla_items(&saved).unwrap(), 3);
    for (form, tag) in selected.iter().zip([*b"ARMO", *b"WEAP", *b"AMMO"]) {
        assert_eq!(world.record(*form).unwrap().tag, Tag(tag));
        assert!(world.restored_item_extra(*form).is_some());
    }
}
