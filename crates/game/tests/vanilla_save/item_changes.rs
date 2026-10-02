// SPDX-License-Identifier: GPL-3.0-or-later

//! Restored item records remain visible through runtime queries and SOEW.

use super::*;

fn item_order() -> LoadOrder {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"INGR"), 0x800).zsub(Tag(*b"FULL"), "Known herb"));
    base.push_top(
        Record::new(Tag(*b"ALCH"), 0x801)
            .zsub(Tag(*b"FULL"), "Old potion")
            .sub(Tag(*b"ENIT"), {
                let mut bytes = 10_i32.to_le_bytes().to_vec();
                bytes.extend(0_u32.to_le_bytes());
                bytes.extend(0_u32.to_le_bytes());
                bytes.extend(0.0_f32.to_le_bytes());
                bytes.extend(0_u32.to_le_bytes());
                bytes
            }),
    );
    base.push_top(Record::new(Tag(*b"ENCH"), 0x802));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order
        .add("A.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    order
        .add("B.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    order
        .add(
            "LIGHT.esl",
            Plugin::new(FileHeader {
                flags: flags::LIGHT_MASTER,
                ..Default::default()
            }),
        )
        .unwrap();
    order
}

fn item_save() -> Save {
    let mut save = save();
    save.plugins = ["Skyrim.esm"].map(WString::from_text).to_vec();
    save.light_plugins.clear();
    save.form_id_array = vec![0x800, 0x801, 0x802];
    save.change_forms = vec![
        change_form(16, 1 | (1 << 31), 1),
        change_form(26, 1 | 2 | 4, 2),
        change_form(48, 1 | 4, 3),
    ];
    save
}

fn change_form(kind: u8, flags: u32, form_index: u32) -> soe_save::ChangeForm {
    soe_save::ChangeForm {
        form: reference(form_index),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66].to_vec(),
    }
}

#[test]
fn sav_38_item_records_apply_flags_markers_and_alchemy_queries_atomically() {
    let game = Game::new(item_order());
    let save = item_save();
    assert_eq!(game.world_mut().restore_vanilla_items(&save).unwrap(), 3);

    let ingredient = id(0, 0x800);
    let alchemy = id(0, 0x801);
    let enchantment = id(0, 0x802);
    assert_eq!(game.world().record(ingredient).unwrap().flags, 0x4433_2211);
    assert_eq!(
        game.world().name_of(ingredient).as_deref(),
        Some("Known herb")
    );
    assert_eq!(game.world().name_of(alchemy).as_deref(), Some("Old potion"));
    assert_eq!(game.world().ingestible(alchemy).unwrap().data.value, 10);
    assert_eq!(game.world().record(enchantment).unwrap().flags, 0x4433_2211);
    assert_eq!(
        game.world()
            .restored_item_extra(ingredient)
            .unwrap()
            .form_flags,
        Some((0x4433_2211, 0x6655,))
    );
    assert!(
        game.world()
            .restored_item_extra(ingredient)
            .unwrap()
            .ingredient_use_changed
    );
    assert!(
        game.world()
            .restored_item_extra(alchemy)
            .unwrap()
            .value_changed
    );
    assert!(
        game.world()
            .restored_item_extra(alchemy)
            .unwrap()
            .fullname_changed
    );
    assert!(
        game.world()
            .restored_item_extra(enchantment)
            .unwrap()
            .fullname_changed
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(item_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(
        loaded.world().record(ingredient).unwrap().flags,
        0x4433_2211
    );
    assert_eq!(
        loaded.world().name_of(alchemy).as_deref(),
        Some("Old potion")
    );
    assert_eq!(loaded.world().ingestible(alchemy).unwrap().data.value, 10);
    assert_eq!(
        loaded.world().restored_item_extra(alchemy),
        game.world().restored_item_extra(alchemy)
    );
}

#[test]
fn sav_38_item_restore_rejects_duplicate_late_change_without_partial_overrides() {
    let game = Game::new(item_order());
    let mut save = item_save();
    save.change_forms[2].form = reference(2);
    save.change_forms[2].type_field = 26;
    let before = game.world().name_of(id(0, 0x800));
    assert!(
        game.world_mut()
            .restore_vanilla_items(&save)
            .unwrap_err()
            .to_string()
            .contains("duplicate or already overridden item")
    );
    assert_eq!(game.world().name_of(id(0, 0x800)), before);
    assert!(game.world().restored_item_extra(id(0, 0x800)).is_none());
}

#[test]
fn sav_38_installed_skyrim_item_records_accept_saved_change_markers() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping vanilla item record restore check");
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    let selected: Vec<_> = [*b"INGR", *b"ALCH", *b"ENCH"]
        .into_iter()
        .map(|tag| {
            plugin
                .records
                .iter()
                .find(|record| record.tag == Tag(tag))
                .unwrap_or_else(|| {
                    panic!("Skyrim.esm is missing {}", String::from_utf8_lossy(&tag))
                })
                .form_id
        })
        .collect();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut save = item_save();
    save.form_id_array.clone_from(&selected);
    save.change_forms = [16, 26, 48]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| change_form(kind, 1, index as u32 + 1))
        .collect();
    assert_eq!(World::new(order).restore_vanilla_items(&save).unwrap(), 3);
}
