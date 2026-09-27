// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    plugin::{FileHeader, Plugin}, GlobalFormId, LoadOrder, Record, Tag,
};
use soe_game::{vanilla_save::RestoreError, World};
use soe_save::{Compression, Header, RefId, Save, WString};
use std::sync::Arc;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Adversarial"),
        player_level: 1,
        player_location: WString::default(),
        game_date: WString::default(),
        player_race: WString::default(),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 0.0,
        filetime: 0,
        screenshot_width: 0,
        screenshot_height: 0,
        compression: Compression::None,
        screenshot: vec![],
    };
    let mut bytes = Vec::new();
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    soe_save::FileLocationTable {
        form_id_array_count_offset: at + 8,
        unknown_table_3_offset: at + 16,
        table_1_offset: at,
        table_2_offset: at,
        change_forms_offset: at,
        table_3_offset: at,
        table_1_count: 0,
        table_2_count: 0,
        table_3_count: 0,
        change_form_count: 0,
        unused: [0; 15],
    }
    .write(&mut bytes);
    soe_save::GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.light_plugins.clear();
    save
}

fn list_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"FLST"), 0x800).sub(Tag(*b"LNAM"), 0x820_u32.to_le_bytes()),
    );
    for (object, tag, base) in [
        (0x801, *b"LVLN", 0x830),
        (0x802, *b"LVLI", 0x820),
        (0x803, *b"LVSP", 0x840),
    ] {
        plugin.push_top(
            Record::new(Tag(tag), object)
                .sub(Tag(*b"LLCT"), [1])
                .sub(Tag(*b"LVLO"), entry(base, 1, 1)),
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

fn entry(form: u32, level: u8, count: u16) -> Vec<u8> {
    let mut data = vec![0; 12];
    data[..2].copy_from_slice(&u16::from(level).to_le_bytes());
    data[4..8].copy_from_slice(&form.to_le_bytes());
    data[8..10].copy_from_slice(&count.to_le_bytes());
    data
}

fn list_change(object: u32, kind: u8, additions: &[u32]) -> soe_save::ChangeForm {
    let mut data = 0xA1B2_C3D4_u32.to_le_bytes().to_vec();
    data.extend(0x5678_u16.to_le_bytes());
    if kind == 43 {
        data.extend((additions.len() as u32).to_le_bytes());
        for raw in additions {
            reference(*raw).write(&mut data);
        }
    } else {
        data.push(additions.len() as u8);
        for raw in additions {
            reference(*raw).write(&mut data);
            data.push(2);
            data.extend(3_u16.to_le_bytes());
            data.push(255);
        }
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

fn changes(forms: &[u32], npcs: &[u32], items: &[u32], spells: &[u32]) -> Save {
    let mut saved = save();
    saved.change_forms = vec![
        list_change(0x800, 43, forms),
        list_change(0x801, 44, npcs),
        list_change(0x802, 45, items),
        list_change(0x803, 46, spells),
    ];
    saved
}

#[test]
fn saved_additions_reach_warm_queries_and_rolls_after_engine_reload() {
    let order = Arc::new(list_order());
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.form_list(id(0x800)).unwrap(), &[id(0x820)]);
    for list in [0x801, 0x802, 0x803] {
        assert_eq!(world.leveled_list(id(list)).unwrap().entries.len(), 1);
    }
    world
        .restore_vanilla_lists(&changes(
            &[0x400820, 0],
            &[0x400831],
            &[0x400821],
            &[0x400841],
        ))
        .unwrap();
    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();

    assert_eq!(
        loaded.form_list(id(0x800)).unwrap(),
        &[id(0x820), id(0x820), id(0)]
    );
    for (list, target) in [(0x801, 0x831), (0x802, 0x821), (0x803, 0x841)] {
        let entries = &loaded.leveled_list(id(list)).unwrap().entries;
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[1].form, entries[1].level, entries[1].count), (id(target), 2, 3));
        assert_eq!(
            loaded.restored_list_extra(id(list)).unwrap().entry_chances,
            [255]
        );
    }
    assert_eq!(
        loaded.roll_leveled_items(id(0x802), 2, 1).unwrap(),
        [(id(0x821), 3)].into_iter().collect()
    );
    assert_eq!(
        loaded.select_leveled_spells(id(0x803), 2).unwrap(),
        [id(0x841)].into_iter().collect()
    );
}

#[test]
fn missing_list_record_rejects_the_batch_without_changing_warm_state() {
    let order = list_order();
    let mut world = World::new(order);
    world.form_list(id(0x800)).unwrap();
    for list in [0x801, 0x802, 0x803] {
        world.leveled_list(id(list)).unwrap();
    }
    let before = world.save_state();
    let mut saved = changes(
        &[0x400821],
        &[0x400831],
        &[0x400821],
        &[0x400841],
    );
    saved.change_forms[3].form = reference(0x400999);

    assert!(matches!(
        world.restore_vanilla_lists(&saved),
        Err(RestoreError::List { .. })
    ));
    assert_eq!(world.save_state(), before);
    assert_eq!(world.form_list(id(0x800)).unwrap(), &[id(0x820)]);
    for list in [0x801, 0x802, 0x803] {
        assert_eq!(world.leveled_list(id(list)).unwrap().entries.len(), 1);
    }
}
