// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved placements must resolve identities before any world mutation.

use super::*;
use soe_save::{
    reference::{flags as change_flags, PlayerLocation},
    ChangeForm,
};

pub(crate) fn fixture_order() -> LoadOrder {
    fixture_order_with_flags(0)
}

pub(crate) fn fixture_order_with_flags(flags: u32) -> LoadOrder {
    let mut master = Plugin::new(FileHeader::default());
    for (tag, object) in [
        (b"NPC_", 7),
        (b"STAT", 0x900),
        (b"STAT", 0x901),
        (b"CELL", 0x800),
        (b"WRLD", 0x801),
    ] {
        let mut record = Record::new(Tag(*tag), object);
        if object == 7 {
            record = record.sub(Tag(*b"EDID"), b"Player\0".to_vec());
        }
        master.push_top(record);
    }
    let mut grid = Record::new(Tag(*b"CELL"), 0x802).sub(
        Tag(*b"XCLC"),
        [-1_i32, 0]
            .into_iter()
            .flat_map(i32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    grid.placement.world = Some(0x801);
    master.push_top(grid);
    let mut placed = Record::new(Tag(*b"REFR"), 0xA00)
        .with_flags(flags)
        .sub(Tag(*b"NAME"), 0x900_u32.to_le_bytes().to_vec());
    placed.placement.cell = Some(0x800);
    master.push_top(placed);
    for (tag, object, base) in [(b"PCON", 0xA02, 0x900_u32), (b"PARW", 0xA03, 0x901_u32)] {
        let mut projectile = Record::new(Tag(*tag), object)
            .sub(Tag(*b"NAME"), base.to_le_bytes().to_vec())
            .sub(Tag(*b"DATA"), vec![0; 24]);
        projectile.placement.cell = Some(0x800);
        master.push_top(projectile);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", master).unwrap();
    for (name, light) in [
        ("Extra.esl", true),
        ("B.esp", false),
        ("A.esp", false),
        ("Light.esl", true),
    ] {
        let mut p = plugin(light);
        p.push_top(Record::new(Tag(*b"STAT"), 0x900));
        order.add(name, p).unwrap();
    }
    order
}

#[test]
fn sav_15_saved_reference_mask_inverts_authored_flags() {
    let mask = soe_esm::plugin::flags::INITIALLY_DISABLED | soe_esm::plugin::flags::PERSISTENT;
    let mut save = fixture();
    set_form_mask(&mut save.change_forms[1], mask, 0x50, 78);

    let mut world = World::new(fixture_order_with_flags(mask));
    let report = world.restore_vanilla_reference_placements(&save).unwrap();
    let restored = world.reference_ref(id(0, 0xA00)).unwrap();
    assert!(restored.enabled);
    assert!(!restored.persistent);
    assert_eq!(
        report.remaining[&id(0, 0xA00)].form_flags,
        Some((mask, 0x50))
    );
    assert!(!world.is_persistent_child(id(0, 0x800), id(0, 0xA00)));

    let bytes = world.save_state();
    let mut loaded = World::new(fixture_order_with_flags(mask));
    loaded.load_state(&bytes).unwrap();
    assert!(loaded.reference_ref(id(0, 0xA00)).unwrap().enabled);
    assert!(!loaded.reference_ref(id(0, 0xA00)).unwrap().persistent);
}

#[test]
fn sav_15_saved_reference_masks_require_skyrim_se_v78_semantics() {
    for invalid_version in [0_u8, 1, 2] {
        let mut save = fixture();
        set_form_mask(
            &mut save.change_forms[1],
            soe_esm::plugin::flags::PERSISTENT,
            0,
            78,
        );
        match invalid_version {
            0 => save.header.version = 11,
            1 => save.form_version = 77,
            _ => save.change_forms[1].version = 77,
        }
        let mut world = World::new(fixture_order());
        let before = world.state_hash();
        let error = world
            .restore_vanilla_reference_placements(&save)
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("matching supported form versions"));
        assert_eq!(world.state_hash(), before);
    }
}

#[test]
fn sav_15_saved_actor_reference_masks_invert_actor_flags() {
    let mask = soe_esm::plugin::flags::INITIALLY_DISABLED | soe_esm::plugin::flags::PERSISTENT;
    let mut save = fixture();
    let player = &mut save.change_forms[0];
    player.change_flags |= change_flags::FORM;
    player.version = 78;
    player.data.splice(
        35..35,
        mask.to_le_bytes().into_iter().chain(0x50_u16.to_le_bytes()),
    );

    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&save).unwrap();
    let player = world.reference_ref(id(0, 0x14)).unwrap();
    assert!(!player.enabled);
    assert!(player.persistent);
}

#[test]
fn sav_15_installed_vanilla_reference_mask_inverts_controlled_authored_flags() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping SAV-15 vanilla-reference case");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).unwrap();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mask = soe_esm::plugin::flags::INITIALLY_DISABLED | soe_esm::plugin::flags::PERSISTENT;
    let record = plugin
        .records_of(Tag(*b"REFR"))
        .find(|record| !record.is_deleted())
        .unwrap();
    let authored = record.flags & mask;

    let mut save = fixture();
    set_form_mask(&mut save.change_forms[1], mask, 0, 78);
    let mut world = World::new(fixture_order_with_flags(authored));
    world.restore_vanilla_reference_placements(&save).unwrap();
    let restored = world.reference_ref(id(0, 0xA00)).unwrap();
    assert_eq!(
        restored.enabled,
        (authored ^ mask) & soe_esm::plugin::flags::INITIALLY_DISABLED == 0
    );
    assert_eq!(
        restored.persistent,
        (authored ^ mask) & soe_esm::plugin::flags::PERSISTENT != 0
    );
}

#[test]
fn sav_15_saved_mask_inverts_enable_and_persistence_bits() {
    let mask = soe_esm::plugin::flags::INITIALLY_DISABLED | soe_esm::plugin::flags::PERSISTENT;
    for authored in [
        0,
        soe_esm::plugin::flags::INITIALLY_DISABLED,
        soe_esm::plugin::flags::PERSISTENT,
        mask,
    ] {
        let mut save = fixture();
        set_form_mask(&mut save.change_forms[1], mask, 0x50, 78);
        let mut world = World::new(fixture_order_with_flags(authored));
        world.restore_vanilla_reference_placements(&save).unwrap();
        let restored = world.reference_ref(id(0, 0xA00)).unwrap();
        assert_eq!(
            restored.enabled,
            (authored ^ mask) & soe_esm::plugin::flags::INITIALLY_DISABLED == 0
        );
        assert_eq!(
            restored.persistent,
            (authored ^ mask) & soe_esm::plugin::flags::PERSISTENT != 0
        );
    }
}

#[test]
fn sav_15_saved_mask_does_not_enable_deleted_winners() {
    let mask = soe_esm::plugin::flags::INITIALLY_DISABLED;
    let authored = soe_esm::plugin::flags::DELETED;
    let mut save = fixture();
    set_form_mask(&mut save.change_forms[1], mask, 0, 78);
    let mut world = World::new(fixture_order_with_flags(authored));
    world.restore_vanilla_reference_placements(&save).unwrap();
    assert!(!world.reference_ref(id(0, 0xA00)).unwrap().enabled);
}

#[test]
fn sav_54_reference_mask_words_invert_enable_and_persistence_across_supported_versions() {
    let disabled = soe_esm::plugin::flags::INITIALLY_DISABLED;
    let persistent = soe_esm::plugin::flags::PERSISTENT;
    for (save_version, form_version) in [(12, 77), (12, 78), (13, 77), (13, 78)] {
        for authored in [0, disabled, persistent, disabled | persistent] {
            for (mask, set_word) in [
                (0, 0),
                (disabled, 0),
                (persistent, 0),
                (disabled | persistent, 0),
                (0, u16::MAX),
                (u32::MAX, 0x0050),
            ] {
                let mut save = fixture();
                save.header.version = save_version;
                save.form_version = form_version;
                set_form_mask(&mut save.change_forms[1], mask, set_word, form_version);
                let mut before_world = World::new(fixture_order_with_flags(authored));
                before_world.reference(id(0, 0xA00));
                let before = before_world.reference_ref(id(0, 0xA00)).unwrap();
                assert_eq!(before.enabled, authored & disabled == 0);
                assert_eq!(before.persistent, authored & persistent != 0);

                let mut world = World::new(fixture_order_with_flags(authored));
                let report = world.restore_vanilla_reference_placements(&save).unwrap();
                assert_eq!(
                    report.remaining[&id(0, 0xA00)].form_flags,
                    Some((mask, set_word))
                );

                let reference = world.reference_ref(id(0, 0xA00)).unwrap();
                assert_eq!(reference.enabled, (authored ^ mask) & disabled == 0);
                assert_eq!(reference.persistent, (authored ^ mask) & persistent != 0);
                assert_eq!(
                    world.is_persistent_child(id(0, 0x800), id(0, 0xA00)),
                    (authored ^ mask) & persistent != 0
                );

                let bytes = world.save_state();
                let mut loaded = World::new(fixture_order_with_flags(authored));
                loaded.load_state(&bytes).unwrap();
                let restored = loaded.reference_ref(id(0, 0xA00)).unwrap();
                assert_eq!(restored.enabled, (authored ^ mask) & disabled == 0);
                assert_eq!(restored.persistent, (authored ^ mask) & persistent != 0);
            }
        }
    }
}

#[test]
fn sav_54_installed_reference_mask_words_apply_before_and_after_across_versions() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping SAV-54 installed-record case");
        return;
    };
    let plugin = Plugin::parse(&std::fs::read(data.join("Skyrim.esm")).unwrap()).unwrap();
    let authored = plugin
        .records_of(Tag(*b"REFR"))
        .find(|record| !record.is_deleted())
        .expect("Skyrim.esm contains a live REFR")
        .flags;
    let disabled = soe_esm::plugin::flags::INITIALLY_DISABLED;
    let persistent = soe_esm::plugin::flags::PERSISTENT;

    for (save_version, form_version) in [(12, 77), (12, 78), (13, 77), (13, 78)] {
        for (mask, set_word) in [
            (disabled | persistent, 0),
            (0, u16::MAX),
            (disabled | persistent, 0x0050),
        ] {
            let mut save = fixture();
            save.header.version = save_version;
            save.form_version = form_version;
            set_form_mask(&mut save.change_forms[1], mask, set_word, form_version);

            let mut before_world = World::new(fixture_order_with_flags(authored));
            before_world.reference(id(0, 0xA00));
            let before = before_world.reference_ref(id(0, 0xA00)).unwrap();
            assert_eq!(before.enabled, authored & disabled == 0);
            assert_eq!(before.persistent, authored & persistent != 0);

            let mut after_world = World::new(fixture_order_with_flags(authored));
            after_world
                .restore_vanilla_reference_placements(&save)
                .unwrap();
            let after = after_world.reference_ref(id(0, 0xA00)).unwrap();
            let effective = authored ^ mask;
            assert_eq!(after.enabled, effective & disabled == 0);
            assert_eq!(after.persistent, effective & persistent != 0);
            assert_eq!(
                after_world.is_persistent_child(id(0, 0x800), id(0, 0xA00)),
                effective & persistent != 0
            );
        }
    }
}

#[test]
fn sav_54_every_accepted_form_version_retains_both_mask_words() {
    let target = id(0, 0xA00);
    for save_version in [12, 13, u32::MAX] {
        for form_version in 77..=u8::MAX {
            let mut save = fixture();
            // CONTEXT: Version 77 has no light-plugin table.
            save.form_id_array[1] = 0x0100_0900;
            save.light_plugins.clear();
            save.header.version = save_version;
            save.form_version = form_version;
            set_form_mask(&mut save.change_forms[1], 0x89AB_CDEF, 0xFEDC, form_version);
            let parsed = Save::parse(&save.write().unwrap()).unwrap();
            let mut world = World::new(fixture_order());
            let report = world.restore_vanilla_reference_placements(&parsed).unwrap();
            assert_eq!(
                report.remaining[&target].form_flags,
                Some((0x89AB_CDEF, 0xFEDC)),
                "ESS {save_version}, form {form_version}"
            );
        }
    }
}

#[test]
fn sav_54_engine_isolates_each_mask_word_at_every_accepted_form_version() {
    let disabled = soe_esm::plugin::flags::INITIALLY_DISABLED;
    let persistent = soe_esm::plugin::flags::PERSISTENT;
    let target = id(0, 0xA00);
    // CONTEXT: These expectations describe this engine, not an independent
    // observation of the original runtime's loading operation.
    for save_version in [12, 13, u32::MAX] {
        for form_version in 77..=u8::MAX {
            for (authored, enabled, retained) in [
                (0, true, false),
                (disabled, false, false),
                (persistent, true, true),
                (disabled | persistent, false, true),
            ] {
                for (mask, second, expected) in [
                    (0, 0, (enabled, retained)),
                    (disabled, 0, (!enabled, retained)),
                    (persistent, 0, (enabled, !retained)),
                    (disabled | persistent, 0, (!enabled, !retained)),
                    (0, disabled as u16, (enabled, retained)),
                    (0, persistent as u16, (enabled, retained)),
                    (0, u16::MAX, (enabled, retained)),
                ] {
                    let mut save = fixture();
                    save.form_id_array[1] = 0x0100_0900;
                    save.light_plugins.clear();
                    save.header.version = save_version;
                    save.form_version = form_version;
                    set_form_mask(&mut save.change_forms[1], mask, second, form_version);
                    let parsed = Save::parse(&save.write().unwrap()).unwrap();
                    let mut world = World::new(fixture_order_with_flags(authored));
                    world.restore_vanilla_reference_placements(&parsed).unwrap();
                    let state = world.reference_ref(target).unwrap();
                    assert_eq!(
                        (state.enabled, state.persistent),
                        expected,
                        "ESS {save_version}, form {form_version}, authored {authored:#x}, words {mask:#x}/{second:#x}"
                    );
                }
            }
        }
    }
}

#[test]
fn sav_54_late_unsupported_mask_version_leaves_the_world_unchanged() {
    for (save_version, form_version, change_version) in [
        (11, 78, 78),
        (12, 76, 76),
        (12, 78, 77),
        (12, 77, 78),
        (13, u8::MAX, 78),
    ] {
        let mut save = fixture();
        save.header.version = save_version;
        save.form_version = form_version;
        let late = save.change_forms.last_mut().unwrap();
        let boundary = late
            .reference_header(form_version)
            .unwrap()
            .unwrap()
            .bytes_consumed;
        late.change_flags |= change_flags::FORM;
        late.version = change_version;
        late.data.splice(
            boundary..boundary,
            u32::MAX
                .to_le_bytes()
                .into_iter()
                .chain(u16::MAX.to_le_bytes()),
        );
        let mut world = World::new(fixture_order());
        let before = world.save_state();
        let error = world
            .restore_vanilla_reference_placements(&save)
            .unwrap_err();
        assert!(
            matches!(error, RestoreError::WorldReference { .. })
                && error
                    .to_string()
                    .contains("matching supported form versions"),
            "ESS {save_version}, form {form_version}, change {change_version}: {error}"
        );
        assert_eq!(world.save_state(), before);
    }
}

fn change(
    raw: u32,
    actor: bool,
    flags: u32,
    parent: u32,
    position: [f32; 3],
    base: Option<u32>,
) -> ChangeForm {
    let mut data = vec![];
    if raw >> 22 == 2 || flags & change_flags::MOVE != 0 {
        reference(parent).write(&mut data);
        for v in position.into_iter().chain([0.0, 0.5, -0.0]) {
            data.extend(v.to_le_bytes());
        }
    }
    if raw >> 22 == 2 {
        data.push(0x12);
        reference(base.unwrap()).write(&mut data);
    }
    if actor {
        data.extend([0; 8]);
    }
    if flags & change_flags::BASE != 0 {
        reference(base.unwrap()).write(&mut data);
    }
    if flags & change_flags::SCALE != 0 {
        data.extend(1.5_f32.to_le_bytes());
    }
    ChangeForm {
        form: reference(raw),
        change_flags: flags,
        type_field: u8::from(actor),
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

pub(crate) fn set_form_mask(change: &mut ChangeForm, mask: u32, second: u16, version: u8) {
    change.change_flags |= change_flags::FORM;
    change.version = version;
    change.data.splice(
        27..27,
        mask.to_le_bytes().into_iter().chain(second.to_le_bytes()),
    );
}

pub(crate) fn fixture() -> Save {
    let mut save = save();
    save.form_id_array = vec![0x0100_0900, 0xFE00_0900];
    save.table_1.push(GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF000804,
            world: reference(0),
            coordinates: [0; 2],
            cell_or_world: reference(0x400800),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    save.change_forms = vec![
        change(
            0x400014,
            true,
            change_flags::MOVE,
            0x400800,
            [1.0, 2.0, 3.0],
            None,
        ),
        change(
            0x400A00,
            false,
            change_flags::MOVE | change_flags::BASE | change_flags::SCALE,
            0x400801,
            [-1.0, 0.0, 4.0],
            Some(1),
        ),
        change(0x800800, false, 0, 0x400800, [4.0, 5.0, 6.0], Some(2)),
    ];
    save
}

#[test]
fn references_restore_positions_bases_cells_and_allocate_without_reusing_saved_ids() {
    let save = fixture();
    let mut world = World::new(fixture_order());
    let report = world.restore_vanilla_reference_placements(&save).unwrap();
    assert_eq!((report.references, report.created), (3, 1));
    assert!(
        report.missing_bases.is_empty()
            && report.missing_cells.is_empty()
            && report.unplaced.is_empty()
    );
    assert_eq!(world.player(), id(0, 0x14));
    assert_eq!(world.loaded_cell(), Some(id(0, 0x800)));
    let moved = world.reference_ref(id(0, 0xA00)).unwrap();
    assert_eq!(moved.position, [-1.0, 0.0, 4.0]);
    assert_eq!(moved.cell, Some(id(0, 0x802)));
    assert_eq!(moved.base, Some(id(3, 0x900)));
    assert_eq!(moved.scale, 1.5);
    let created = world.reference_ref(id(DYNAMIC_FILE, 0x800)).unwrap();
    assert_eq!(created.base, Some(id(4, 0x900)));
    assert_eq!(created.position, [4.0, 5.0, 6.0]);
    assert!(world.pending.is_empty());
    let bytes = world.save_state();
    let mut restored = World::new(fixture_order());
    restored.load_state(&bytes).unwrap();
    assert_eq!(world.state_hash(), restored.state_hash());
    assert_eq!(
        restored.reference_ref(id(0, 0xA00)).unwrap().cell,
        Some(id(0, 0x802))
    );
    assert_eq!(
        restored.place_at(id(0, 0x900), id(0, 0x14), false),
        id(DYNAMIC_FILE, 0x804)
    );
    assert_eq!(
        world.place_at(id(0, 0x900), id(0, 0x14), false),
        id(DYNAMIC_FILE, 0x804)
    );
}

#[test]
fn every_saved_created_namespace_entry_reserves_allocation_space() {
    let mut save = fixture();
    save.form_id_array.push(0xFF000900);
    save.change_forms.push(ChangeForm {
        form: reference(0x800A00),
        change_flags: 0,
        type_field: 9,
        version: 0,
        uncompressed_len: 0,
        data: vec![],
    });
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&save).unwrap();
    assert_eq!(
        world.place_at(id(0, 0x900), id(0, 0x14), false),
        id(DYNAMIC_FILE, 0xA01)
    );
}

#[test]
fn sav_17_created_base_identity_is_reserved_before_placement_and_inventory() {
    let mut save = fixture();
    let created_base = 0x800902;
    let mut object_table = Vec::new();
    for (object, effect) in [
        (0x900, None),
        (0x901, None),
        (0x902, Some(0x400900)),
        (0x903, None),
    ] {
        object_table.push(4);
        reference(0x800000 | object).write(&mut object_table);
        object_table.extend(7_u32.to_le_bytes());
        if let Some(effect) = effect {
            object_table.push(4);
            reference(effect).write(&mut object_table);
            object_table.extend(12.5_f32.to_le_bytes());
            object_table.extend(30_u32.to_le_bytes());
            object_table.extend(2_u32.to_le_bytes());
            object_table.extend(4.25_f32.to_le_bytes());
        } else {
            object_table.push(0);
        }
    }
    save.table_1.push(GlobalData {
        kind: GlobalData::CREATED_OBJECTS,
        data: object_table,
    });

    let mut created_reference = change(
        0x800800,
        false,
        1 << 5,
        0x400800,
        [4.0, 5.0, 6.0],
        Some(created_base),
    );
    created_reference.data.extend([4]);
    reference(created_base).write(&mut created_reference.data);
    created_reference.data.extend(2_i32.to_le_bytes());
    created_reference.data.push(0);
    save.change_forms[2] = created_reference;

    let mut world = World::new(fixture_order());
    let report = world.restore_vanilla_reference_placements(&save).unwrap();
    let base = id(DYNAMIC_FILE, 0x902);
    assert!(!report.missing_bases.contains(&base));
    for (object, tag) in [
        (0x900, Tag(*b"ENCH")),
        (0x901, Tag(*b"ENCH")),
        (0x902, Tag(*b"ALCH")),
    ] {
        assert_eq!(world.record(id(DYNAMIC_FILE, object)).unwrap().tag, tag);
    }
    let record = world.record(base).unwrap();
    assert_eq!(record.tag, Tag(*b"ALCH"));
    assert_eq!(record.find(Tag(*b"ENIT")), Some(&[0; 20][..]));
    assert_eq!(
        world.resolve_from(
            base,
            u32::from_le_bytes(record.find(Tag(*b"EFID")).unwrap().try_into().unwrap())
        ),
        Some(id(0, 0x900))
    );
    assert_eq!(
        world.reference_ref(id(DYNAMIC_FILE, 0x800)).unwrap().base,
        Some(base)
    );
    assert_eq!(
        world.place_at(id(0, 0x900), id(0, 0x14), false),
        id(DYNAMIC_FILE, 0x904)
    );

    let inventory = world.restore_vanilla_inventories(&save).unwrap();
    assert_eq!(inventory.references, 3);
    assert!(!inventory.missing_items.contains(&base));
    assert_eq!(world.item_count(id(DYNAMIC_FILE, 0x800), base), 2);

    let bytes = world.save_state();
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(loaded.record(base).unwrap().tag, Tag(*b"ALCH"));
    assert_eq!(loaded.item_count(id(DYNAMIC_FILE, 0x800), base), 2);
    let loaded_record = loaded.record(base).unwrap();
    let effect = u32::from_le_bytes(
        loaded_record
            .find(Tag(*b"EFID"))
            .unwrap()
            .try_into()
            .unwrap(),
    );
    assert_eq!(loaded.resolve_from(base, effect), Some(id(0, 0x900)));
}

#[test]
fn missing_placement_dependencies_and_unapplied_state_are_reported() {
    let mut save = fixture();
    save.change_forms[2] = change(
        0x800800,
        false,
        0,
        0x400ABC,
        [4.0, 5.0, 6.0],
        Some(0x800FFF),
    );
    save.change_forms[2].data.extend([1, 2, 3]);
    let mut world = World::new(fixture_order());
    let report = world.restore_vanilla_reference_placements(&save).unwrap();
    assert!(report.missing_bases.contains(&id(DYNAMIC_FILE, 0xFFF)));
    assert!(report.unplaced.contains(&id(DYNAMIC_FILE, 0x800)));
    assert_eq!(report.missing_cells.len(), 1);
    let remainder = &report.remaining[&id(DYNAMIC_FILE, 0x800)];
    assert_eq!(remainder.suffix_bytes, 3);
    assert_eq!(remainder.creation_flags, Some(0x12));
}

#[test]
fn malformed_late_entries_and_inconsistent_player_data_leave_whole_game_unchanged() {
    let original = fixture();
    let mut cases = vec![];
    let mut duplicate = original.clone();
    duplicate
        .change_forms
        .push(duplicate.change_forms[2].clone());
    cases.push(duplicate);
    let mut truncated = original.clone();
    truncated.change_forms[2].data.pop();
    cases.push(truncated);
    let mut mismatch = original.clone();
    mismatch.change_forms[0].data[3..7].copy_from_slice(&9_f32.to_le_bytes());
    cases.push(mismatch);
    let mut nonfinite = original.clone();
    nonfinite.change_forms[2].data[3..7].copy_from_slice(&f32::NAN.to_le_bytes());
    cases.push(nonfinite);
    let mut bad_parent = original.clone();
    bad_parent.change_forms[2].data[..3].copy_from_slice(&[0x40, 0x09, 0]);
    cases.push(bad_parent);
    let mut exhausted = original.clone();
    exhausted.form_id_array.push(0xFFFFFFFF);
    cases.push(exhausted);
    let mut absent = original.clone();
    absent.table_1.clear();
    cases.push(absent);
    for save in cases {
        let game = Game::new(fixture_order());
        let before = game.save_state().unwrap();
        assert!(game
            .world_mut()
            .restore_vanilla_reference_placements(&save)
            .is_err());
        assert_eq!(game.save_state().unwrap(), before);
    }
    let mut world = World::new(fixture_order());
    world.player();
    let before = world.save_state();
    assert!(world
        .restore_vanilla_reference_placements(&original)
        .is_err());
    assert_eq!(world.save_state(), before);
}

#[test]
fn installed_saved_reference_placements_restore_and_persist() {
    let (Some(_data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping reference placement survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let order = Arc::new(order);
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut total, mut created, mut missing_bases, mut missing_cells, mut unplaced) =
        (0, 0, 0, 0, 0);
    let (mut quests, mut unresolved_quest_forms) = (0, 0);
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        let report = world
            .restore_vanilla_reference_placements(&save)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let mut count = 0;
        for form in &save.change_forms {
            let Some(header) = form.reference_header(save.form_version).unwrap() else {
                continue;
            };
            let id = mapping.reference(form.form).unwrap().unwrap();
            let state = world.reference_ref(id).unwrap();
            if let Some(placement) = header.initial.placement() {
                assert_eq!(
                    state.position.map(f32::to_bits),
                    placement.position.map(f32::to_bits)
                );
                assert_eq!(
                    state.rotation.map(f32::to_bits),
                    placement.rotation.map(f32::to_bits)
                );
            }
            if let Some(base) = header.base {
                assert_eq!(state.base, mapping.reference(base).unwrap());
            }
            assert_eq!(report.remaining[&id].suffix_bytes, header.remaining_bytes);
            count += 1;
        }
        assert_eq!(count, report.references);
        world.restore_vanilla_globals(&save).unwrap();
        let quest_report = world.restore_vanilla_quests(&save).unwrap();
        assert!(quest_report
            .unresolved_forms
            .iter()
            .all(|id| id.file == DYNAMIC_FILE));
        assert!(quest_report
            .unresolved_forms
            .iter()
            .all(|id| !report.remaining.contains_key(id)));
        quests += quest_report.quests;
        unresolved_quest_forms += quest_report.unresolved_forms.len();
        assert!(world.pending.is_empty());
        let bytes = world.save_state();
        let mut restored = World::from_shared_order(Arc::clone(&order));
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.state_hash(), world.state_hash());
        for id in report.remaining.keys() {
            let old = world.reference_ref(*id).unwrap();
            let new = restored.reference_ref(*id).unwrap();
            assert_eq!(
                old.position.map(f32::to_bits),
                new.position.map(f32::to_bits)
            );
            assert_eq!((old.base, old.cell), (new.base, new.cell));
        }
        total += report.references;
        created += report.created;
        missing_bases += report.missing_bases.len();
        missing_cells += report.missing_cells.len();
        unplaced += report.unplaced.len();
    }
    assert!(total > 100000);
    eprintln!(
        "{} saves: {total} placements, {created} created, {missing_bases} missing bases, {missing_cells} missing cells, {unplaced} unplaced references",
        paths.len()
    );
    eprintln!(
        "{quests} quests restored after placement/global passes, {unresolved_quest_forms} remaining unresolved quest identities"
    );
}
