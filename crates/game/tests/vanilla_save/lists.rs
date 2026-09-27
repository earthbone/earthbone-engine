// SPDX-License-Identifier: GPL-3.0-or-later

//! SAV-52 list changes reach both record readers and runtime selectors.

use super::*;

fn entry(form: u32, level: u16) -> Vec<u8> {
    let mut data = vec![0; 12];
    data[..2].copy_from_slice(&level.to_le_bytes());
    data[4..8].copy_from_slice(&form.to_le_bytes());
    data[8..10].copy_from_slice(&1_u16.to_le_bytes());
    data
}

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"FLST"), 0x800).sub(Tag(*b"LNAM"), 0x820_u32.to_le_bytes()));
    for (object, tag, old) in [
        (0x801, *b"LVLN", 0x830),
        (0x802, *b"LVLI", 0x820),
        (0x803, *b"LVSP", 0x840),
    ] {
        plugin.push_top(
            Record::new(Tag(tag), object)
                .sub(Tag(*b"LLCT"), [1])
                .sub(Tag(*b"LVLO"), entry(old, 99)),
        );
    }
    for (object, tag) in [
        (0x820, *b"MISC"),
        (0x821, *b"MISC"),
        (0x830, *b"NPC_"),
        (0x831, *b"NPC_"),
        (0x840, *b"SPEL"),
        (0x841, *b"SPEL"),
    ] {
        plugin.push_top(Record::new(Tag(tag), object));
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, added: u32, chance: u8) -> soe_save::ChangeForm {
    let mut data = 0xA5A5_0001_u32.to_le_bytes().to_vec();
    data.extend(0x7B7B_u16.to_le_bytes());
    if kind == 43 {
        data.extend(2_u32.to_le_bytes());
        reference(0x400000 | added).write(&mut data);
        reference(0).write(&mut data);
    } else {
        data.push(1);
        reference(0x400000 | added).write(&mut data);
        data.push(1);
        data.extend(1_u16.to_le_bytes());
        data.push(chance);
    }
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: 1 | (1 << 31),
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
        change(0x800, 43, 0x821, 0),
        change(0x801, 44, 0x831, 17),
        change(0x802, 45, 0x821, 31),
        change(0x803, 46, 0x841, 47),
    ];
    saved
}

#[test]
fn sav_52_all_four_list_changes_reach_warm_runtime_and_record_queries_after_reload() {
    let order = Arc::new(fixture_order());
    let mut world = World::from_shared_order(order.clone());
    for object in [0x801, 0x802, 0x803] {
        assert_eq!(world.leveled_list(id(0, object)).unwrap().entries.len(), 1);
    }
    assert_eq!(world.form_list(id(0, 0x800)).unwrap().len(), 1);
    assert_eq!(world.restore_vanilla_lists(&fixture()).unwrap(), 4);
    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(
        loaded.form_list(id(0, 0x800)).unwrap(),
        &[id(0, 0x820), id(0, 0x821)]
    );
    assert_eq!(
        loaded
            .record(id(0, 0x800))
            .unwrap()
            .all(Tag(*b"LNAM"))
            .count(),
        3
    );
    for (list, form, chance) in [(0x801, 0x831, 17), (0x802, 0x821, 31), (0x803, 0x841, 47)] {
        let state = loaded.leveled_list(id(0, list)).unwrap();
        assert_eq!(state.entries[1].form, id(0, form));
        assert_eq!(state.base_entries, 2);
        assert_eq!(
            loaded
                .record(id(0, list))
                .unwrap()
                .all(Tag(*b"LVLO"))
                .count(),
            2
        );
        let extra = loaded.restored_list_extra(id(0, list)).unwrap();
        assert_eq!(extra.entry_chances, [chance]);
        assert_eq!(extra.form_flags, Some((0xA5A5_0001, 0x7B7B)));
    }
    assert_eq!(
        loaded
            .roll_leveled_items(id(0, 0x802), 1, 1)
            .unwrap()
            .get(&id(0, 0x821)),
        Some(&1)
    );
    assert_eq!(
        loaded.select_leveled_spells(id(0, 0x803), 1).unwrap(),
        [id(0, 0x841)].into_iter().collect()
    );
}

#[test]
fn sav_52_late_invalid_change_preserves_all_lists_atomically() {
    let mut world = World::new(fixture_order());
    world.form_list(id(0, 0x800)).unwrap();
    let before = world.save_state();
    let mut saved = fixture();
    saved.change_forms[3].form = reference(0x400999);
    assert!(world.restore_vanilla_lists(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(world.form_list(id(0, 0x800)).unwrap(), &[id(0, 0x820)]);
}

#[test]
fn sav_52_duplicate_change_rejects_without_partial_application() {
    let mut world = World::new(fixture_order());
    let before = world.save_state();
    let mut saved = fixture();
    saved.change_forms.push(saved.change_forms[0].clone());
    assert!(world.restore_vanilla_lists(&saved).is_err());
    assert_eq!(world.save_state(), before);
}

#[test]
fn sav_52_flags_only_and_empty_section_keep_distinct_list_state() {
    let mut saved = fixture();
    saved.change_forms[0].change_flags = 1;
    saved.change_forms[0].data.truncate(6);
    saved.change_forms[1].data.truncate(6);
    saved.change_forms[1].data.push(0);
    let mut world = World::new(fixture_order());
    world.restore_vanilla_lists(&saved).unwrap();
    assert_eq!(world.form_list(id(0, 0x800)).unwrap(), &[id(0, 0x820)]);
    assert_eq!(world.leveled_list(id(0, 0x801)).unwrap().entries.len(), 1);
    assert_eq!(
        world.restored_list_extra(id(0, 0x800)).unwrap().form_flags,
        Some((0xA5A5_0001, 0x7B7B))
    );
    assert!(
        !world
            .restored_list_extra(id(0, 0x800))
            .unwrap()
            .section_present
    );
    assert!(
        world
            .restored_list_extra(id(0, 0x801))
            .unwrap()
            .section_present
    );
    assert_eq!(
        world
            .restored_list_extra(id(0, 0x801))
            .unwrap()
            .entry_chances,
        Vec::<u8>::new()
    );
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert!(
        !loaded
            .restored_list_extra(id(0, 0x800))
            .unwrap()
            .section_present
    );
    assert!(
        loaded
            .restored_list_extra(id(0, 0x801))
            .unwrap()
            .section_present
    );
    assert_eq!(
        loaded
            .record(id(0, 0x800))
            .unwrap()
            .all(Tag(*b"LNAM"))
            .count(),
        1
    );
}

#[test]
fn sav_52_entry_count_overflow_rejects_all_prepared_changes() {
    let mut order = fixture_order();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    let mut list = Record::new(Tag(*b"LVSP"), 0x803).sub(Tag(*b"LLCT"), [255]);
    for _ in 0..255 {
        list = list.sub(Tag(*b"LVLO"), entry(0x840, 1));
    }
    plugin.push_top(list);
    order.add("Patch.esp", plugin).unwrap();
    let mut world = World::new(order);
    let before = world.save_state();
    assert!(world.restore_vanilla_lists(&fixture()).is_err());
    assert_eq!(world.save_state(), before);
}

#[test]
fn sav_52_invalid_warm_cache_rejects_before_record_override() {
    let mut world = World::new(fixture_order());
    world.leveled_list(id(0, 0x803)).unwrap().base_entries = 2;
    let before = world.save_state();
    assert!(world.restore_vanilla_lists(&fixture()).is_err());
    assert_eq!(world.save_state(), before);
}

#[test]
fn sav_52_installed_vanilla_list_records_accept_saved_additions() {
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
    let tags = [*b"FLST", *b"LVLN", *b"LVLI", *b"LVSP"];
    let selected: Vec<_> = tags
        .into_iter()
        .map(|tag| {
            order
                .winners()
                .find(|(id, rref)| {
                    let record = order.record(*rref);
                    id.file == 0
                        && record.tag == Tag(tag)
                        && !record.is_deleted()
                        && record.all(Tag(*b"LVLO")).count() < 255
                })
                .map(|(id, rref)| {
                    (
                        id,
                        order
                            .record(rref)
                            .all(if tag == *b"FLST" {
                                Tag(*b"LNAM")
                            } else {
                                Tag(*b"LVLO")
                            })
                            .count(),
                    )
                })
                .unwrap()
        })
        .collect();
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.change_forms = selected
        .iter()
        .enumerate()
        .map(|(index, (id, _))| change(id.object, 43 + index as u8, 0, 0))
        .collect();
    let mut world = World::new(order);
    assert_eq!(world.restore_vanilla_lists(&saved).unwrap(), 4);
    for (index, (id, before)) in selected.iter().enumerate() {
        let tag = if index == 0 {
            Tag(*b"LNAM")
        } else {
            Tag(*b"LVLO")
        };
        assert_eq!(
            world.record(*id).unwrap().all(tag).count(),
            before + if index == 0 { 2 } else { 1 }
        );
    }
}
