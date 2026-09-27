// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved base-activator changes update records used by runtime queries.

use super::*;

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, tag, flags, name) in [
        (0x800, *b"ACTI", 0x100, "Activator"),
        (0x801, *b"TACT", 0x200, "Talking Activator"),
        (0x802, *b"DOOR", 0x400, "Door"),
    ] {
        plugin.push_top(
            Record::new(Tag(tag), object)
                .with_flags(flags)
                .sub(Tag(*b"EDID"), format!("Test{object:03}\0").into_bytes())
                .sub(Tag(*b"FULL"), format!("{name}\0").into_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(
    object: u32,
    record_type: u8,
    flags: u32,
    mask: u32,
    unknown: u16,
) -> soe_save::ChangeForm {
    let mut data = mask.to_le_bytes().to_vec();
    data.extend(unknown.to_le_bytes());
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: record_type,
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
        change(0x800, 10, 0b111, 0x1, 0x1234),
        change(0x801, 11, 0x0080_0007, 0x2, 0x5678),
        change(0x802, 15, 0b111, 0x4, 0x9ABC),
    ];
    saved
}

#[test]
fn sav_32_changes_update_all_live_records_and_queries_after_engine_reload() {
    let order = Arc::new(fixture_order());
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.restore_vanilla_activators(&fixture()).unwrap(), 3);

    for (object, expected_flags, name) in [
        (0x800, 0x101, "Activator"),
        (0x801, 0x202, "Talking Activator"),
        (0x802, 0x404, "Door"),
    ] {
        let form = id(0, object);
        assert_eq!(world.record(form).unwrap().flags, expected_flags);
        assert_eq!(world.name_of(form).as_deref(), Some(name));
    }
    assert_eq!(
        world.restored_activator_extra(id(0, 0x800)).unwrap(),
        &soe_game::world::ActivatorRecordExtra {
            form_flags: Some((1, 0x1234)),
            value_changed: true,
            full_name_changed: true,
            speaker_changed: false,
        }
    );
    assert_eq!(
        world.restored_activator_extra(id(0, 0x801)).unwrap(),
        &soe_game::world::ActivatorRecordExtra {
            form_flags: Some((2, 0x5678)),
            value_changed: true,
            full_name_changed: true,
            speaker_changed: true,
        }
    );

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    for object in [0x800, 0x801, 0x802] {
        let form = id(0, object);
        assert_eq!(loaded.record(form), world.record(form));
        assert_eq!(loaded.name_of(form), world.name_of(form));
        assert_eq!(
            loaded.restored_activator_extra(form),
            world.restored_activator_extra(form)
        );
    }
}

#[test]
fn sav_32_late_invalid_activator_keeps_every_live_record_unchanged() {
    let mut world = World::new(fixture_order());
    let before = world.save_state();
    let mut saved = fixture();
    saved.change_forms[2].form = reference(0x400999);
    assert!(world.restore_vanilla_activators(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(world.record(id(0, 0x800)).unwrap().flags, 0x100);
    assert_eq!(world.record(id(0, 0x801)).unwrap().flags, 0x200);
}

#[test]
fn sav_32_rejects_non_talking_speaker_markers_and_keeps_zero_byte_fields() {
    let saved = fixture();
    for form in &saved.change_forms {
        form.activator_change().unwrap().unwrap();
        assert_eq!(form.payload().unwrap().len(), 6);
    }
    let mut non_talking = saved.change_forms[0].clone();
    non_talking.change_flags |= 1 << 23;
    assert!(non_talking.activator_change().is_err());
}

#[test]
fn sav_32_installed_records_accept_saved_flag_changes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    let mut selected = Vec::new();
    for (type_index, tag) in [(10, *b"ACTI"), (11, *b"TACT"), (15, *b"DOOR")] {
        let record = order.files()[0]
            .plugin
            .records_of(Tag(tag))
            .find(|record| record.form_id <= 0x3F_FFFF)
            .unwrap();
        selected.push(change(record.form_id, type_index, 1, 1, 0));
    }
    saved.change_forms = selected;
    let mut world = World::new(order);
    assert_eq!(world.restore_vanilla_activators(&saved).unwrap(), 3);
}
