// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-46 decoded record changes reach record and faction consumers.

use super::*;

const CLASS: u32 = 0x800;
const FACT: u32 = 0x801;
const RELA: u32 = 0x802;
const OTHER: u32 = 0x803;

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CLAS"), CLASS).sub(Tag(*b"DATA"), [0; 16]));
    plugin.push_top(
        Record::new(Tag(*b"FACT"), FACT)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"XNAM"), [0x04, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
            .sub(Tag(*b"CRVA"), [0; 16]),
    );
    let mut data = vec![0; 16];
    data[0..4].copy_from_slice(&0x800_u32.to_le_bytes());
    data[4..8].copy_from_slice(&0x803_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"RELA"), RELA).sub(Tag(*b"DATA"), data));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OTHER));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, flags: u32, data: Vec<u8>) -> soe_save::ChangeForm {
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

fn changes() -> Save {
    let mut save = save();
    save.plugins.truncate(1);
    save.light_plugins.clear();
    let mut fact = vec![0x81, 0x92, 0xA3, 0xB4, 0xC5, 0xD6];
    fact.push(4); // One reaction in the variable-count encoding
    reference(0x400000 | OTHER).write(&mut fact);
    fact.extend((-7_i32).to_le_bytes());
    fact.extend(3_u32.to_le_bytes());
    fact.extend(0xAABB_CCDDu32.to_le_bytes());
    fact.extend([11_u32, 22, 33, 44].into_iter().flat_map(u32::to_le_bytes));
    let mut created_relationship = Vec::new();
    reference(0x400000 | OTHER).write(&mut created_relationship);
    reference(0x400000 | FACT).write(&mut created_relationship);
    reference(0).write(&mut created_relationship);
    created_relationship.extend(2_i32.to_le_bytes());
    save.change_forms = vec![
        change(CLASS, 30, 3, [1, 2, 3, 4, 5, 6].to_vec()),
        change(FACT, 31, 0x8000_0007, fact),
        change(
            RELA,
            39,
            3,
            [0x91, 0x82, 0x73, 0x64, 0x55, 0x46]
                .into_iter()
                .chain((-3_i32).to_le_bytes())
                .collect(),
        ),
        soe_save::ChangeForm {
            form: RefId::from_bytes([0x80, 0x01, 0x23]),
            change_flags: 2,
            type_field: 39,
            version: 78,
            uncompressed_len: 0,
            data: created_relationship,
        },
    ];
    save
}

#[test]
fn sav_46_record_consumers_restore_atomically_and_survive_engine_save_load() {
    let order = Arc::new(order());
    let mut world = World::from_shared_order(order.clone());
    // Warm the native-facing faction table before restoration.
    assert_eq!(
        world.faction(id(0, FACT)).unwrap().reactions[&id(0, 0x804)],
        0
    );
    assert!(!world.faction(id(0, FACT)).unwrap().tracks_crime);
    assert_eq!(world.restore_vanilla_record_changes(&changes()).unwrap(), 4);
    assert_eq!(world.record(id(0, CLASS)).unwrap().flags, 0x0403_0201);
    assert_eq!(world.record(id(0, FACT)).unwrap().flags, 0xB4A3_9281);
    assert_eq!(world.record(id(0, RELA)).unwrap().flags, 0x6473_8291);
    assert_eq!(
        world.relationship_record_rank(id(0, CLASS), id(0, OTHER)),
        Some(-3)
    );

    let faction_record = world.record(id(0, FACT)).unwrap();
    assert_eq!(
        faction_record.find(Tag(*b"DATA")),
        Some(&0xAABB_CCDD_u32.to_le_bytes()[..])
    );
    assert_eq!(
        faction_record.find(Tag(*b"CRVA")),
        Some(&[11, 0, 0, 0, 22, 0, 0, 0, 33, 0, 0, 0, 44, 0, 0, 0][..])
    );
    let reaction = faction_record.find(Tag(*b"XNAM")).unwrap();
    assert_eq!(&reaction[4..8], &(-7_i32).to_le_bytes());
    assert_eq!(&reaction[8..12], &3_u32.to_le_bytes());
    assert_eq!(
        world.faction(id(0, FACT)).unwrap().reactions[&id(0, OTHER)],
        3
    );
    assert!(world.faction(id(0, FACT)).unwrap().tracks_crime);
    assert_eq!(
        u16::from_le_bytes(
            world
                .record(id(0, RELA))
                .unwrap()
                .find(Tag(*b"DATA"))
                .unwrap()[8..10]
                .try_into()
                .unwrap()
        ),
        (-3_i32) as u16
    );
    let created_id = id(soe_game::world::DYNAMIC_FILE, 0x123);
    let created_data = world
        .record(created_id)
        .unwrap()
        .find(Tag(*b"DATA"))
        .unwrap();
    let parent_raw = u32::from_le_bytes(created_data[0..4].try_into().unwrap());
    let child_raw = u32::from_le_bytes(created_data[4..8].try_into().unwrap());
    assert_eq!(
        world.resolve_from(created_id, parent_raw),
        Some(id(0, OTHER))
    );
    assert_eq!(world.resolve_from(created_id, child_raw), Some(id(0, FACT)));

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(loaded.record(id(0, CLASS)).unwrap().flags, 0x0403_0201);
    assert_eq!(loaded.record(id(0, FACT)).unwrap().flags, 0xB4A3_9281);
    assert_eq!(loaded.record(id(0, RELA)).unwrap().flags, 0x6473_8291);
    assert_eq!(
        loaded.relationship_record_rank(id(0, CLASS), id(0, OTHER)),
        Some(-3)
    );
    assert_eq!(
        loaded.faction(id(0, FACT)).unwrap().reactions[&id(0, OTHER)],
        3
    );
    assert!(loaded.faction(id(0, FACT)).unwrap().tracks_crime);
    assert_eq!(
        loaded
            .record(id(0, RELA))
            .unwrap()
            .find(Tag(*b"DATA"))
            .unwrap()[8..10],
        (-3_i32 as u16).to_le_bytes()
    );
    let created_id = id(soe_game::world::DYNAMIC_FILE, 0x123);
    let created_data = loaded
        .record(created_id)
        .unwrap()
        .find(Tag(*b"DATA"))
        .unwrap();
    let parent_raw = u32::from_le_bytes(created_data[0..4].try_into().unwrap());
    assert_eq!(
        loaded.resolve_from(created_id, parent_raw),
        Some(id(0, OTHER))
    );
}

#[test]
fn sav_46_late_invalid_change_leaves_earlier_records_and_warm_consumers_unchanged() {
    let mut saved = changes();
    saved
        .change_forms
        .push(change(0x900, 39, 2, 1_i32.to_le_bytes().to_vec()));
    let mut world = World::from_shared_order(Arc::new(order()));
    let before = world.save_state();
    assert!(world.restore_vanilla_record_changes(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(
        world.record(id(0, FACT)).unwrap().find(Tag(*b"DATA")),
        Some(&0_u32.to_le_bytes()[..])
    );
}
