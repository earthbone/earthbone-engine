// SPDX-License-Identifier: GPL-3.0-or-later

//! COED selection, runtime extra-data conversion and engine-save persistence.

use super::*;
use soe_game::inventory::{Extra, Field};

const WEAPON: u32 = 0x202;
const NPC: u32 = 0x300;
const FACTION: u32 = 0x301;
const GLOBAL: u32 = 0x302;
const HOLDER: u32 = 0x400;

fn coed(owner: u32, conditional: u32, health: f32) -> Vec<u8> {
    [
        owner.to_le_bytes(),
        conditional.to_le_bytes(),
        health.to_le_bytes(),
    ]
    .concat()
}

fn metadata_world(mut records: Vec<Record>) -> World {
    records.extend([
        Record::new(T(b"WEAP"), WEAPON),
        Record::new(T(b"NPC_"), NPC),
        Record::new(T(b"FACT"), FACTION),
        Record::new(T(b"GLOB"), GLOBAL),
        Record::new(T(b"CONT"), HOLDER + 1),
        Record::new(T(b"REFR"), HOLDER).sub(T(b"NAME"), (HOLDER + 1).to_le_bytes()),
    ]);
    world(records)
}

fn field(kind: u8, value: Field) -> Extra {
    Extra {
        kind,
        fields: vec![value],
    }
}

#[test]
fn prg_15_nested_leaf_owner_global_and_condition_survive_inventory_save_load() {
    let records = || {
        vec![
            list(LIST, 0, 0, &[(1, CHILD, 3)]).sub(T(b"COED"), coed(FACTION, 7, 1.8)),
            list(CHILD, 0, 0, &[(1, WEAPON, 2)]).sub(T(b"COED"), coed(NPC, GLOBAL, 1.1)),
        ]
    };
    let mut world = metadata_world(records());
    let inventory = world.roll_leveled_inventory(id(LIST), 1, 4).unwrap();
    assert_eq!(inventory.count(id(WEAPON)), 24);
    assert_eq!(
        inventory.stacks(id(WEAPON))[0].extras(),
        &[
            field(33, Field::Form(Some(id(NPC)))),
            field(34, Field::Form(Some(id(GLOBAL)))),
            field(37, Field::FloatBits(1.1_f32.to_bits())),
        ]
    );
    world.reference(id(HOLDER)).unwrap().inventory = inventory.clone();
    let saved = world.save_state();
    let hash = world.state_hash();
    let mut restored = metadata_world(records());
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.reference_ref(id(HOLDER)).unwrap().inventory,
        inventory
    );
    assert_eq!(restored.state_hash(), hash);
}

#[test]
fn nested_entry_metadata_is_not_inherited_when_leaf_has_none() {
    for flags in [0, 2, 4] {
        let mut world = metadata_world(vec![
            list(LIST, flags, 0, &[(1, CHILD, 3)]).sub(T(b"COED"), coed(FACTION, 7, 1.8)),
            list(CHILD, 0, 0, &[(1, WEAPON, 2)]),
        ]);
        let inventory = world.roll_leveled_inventory(id(LIST), 1, 2).unwrap();
        assert_eq!(inventory.count(id(WEAPON)), 12);
        assert!(inventory.stacks(id(WEAPON)).is_empty());
    }
}

#[test]
fn distinct_leaf_extras_remain_separate_and_matching_extras_combine() {
    let mut record = list(LIST, 4, 0, &[(1, WEAPON, 1)]).sub(T(b"COED"), coed(FACTION, 3, 1.4));
    record.subrecords.retain(|sub| sub.tag != T(b"LLCT"));
    for health in [1.6, 1.4] {
        let next = list(CHILD, 0, 0, &[(1, WEAPON, 2)]);
        record.subrecords.extend(
            next.subrecords
                .into_iter()
                .filter(|sub| sub.tag == T(b"LVLO")),
        );
        record = record.sub(T(b"COED"), coed(FACTION, 3, health));
    }
    let mut world = metadata_world(vec![record]);
    let inventory = world.roll_leveled_inventory(id(LIST), 1, 2).unwrap();
    assert_eq!(inventory.count(id(WEAPON)), 10);
    let stacks = inventory.stacks(id(WEAPON));
    assert_eq!(stacks.len(), 2);
    assert_eq!((stacks[0].count(), stacks[1].count()), (6, 4));
    assert_eq!(stacks[0].extra(35), Some(&field(35, Field::Word(3))));
    assert_eq!(
        world.roll_leveled_items(id(LIST), 1, 2).unwrap(),
        BTreeMap::from([(id(WEAPON), 10)])
    );
}

#[test]
fn condition_bits_are_direct_and_default_epsilon_is_omitted() {
    for health in [-1.0, -0.1, 0.0, 0.5, 0.9999, 1.0, 1.00005, 1.0001, 1.1, 2.5] {
        let mut world = metadata_world(vec![
            list(LIST, 0, 0, &[(1, WEAPON, 1)]).sub(T(b"COED"), coed(NPC, 0, health)),
        ]);
        let inventory = world.roll_leveled_inventory(id(LIST), 1, 1).unwrap();
        let expected = (health != -1.0 && !(health > 0.9999 && health < 1.0001))
            .then(|| field(37, Field::FloatBits(health.to_bits())));
        assert_eq!(
            inventory.stacks(id(WEAPON))[0].extra(37),
            expected.as_ref(),
            "health={health}"
        );
    }
    let mut world = metadata_world(vec![
        list(LIST, 0, 0, &[(1, A, 1)]).sub(T(b"COED"), coed(FACTION, u32::MAX, 1.8)),
    ]);
    let inventory = world.roll_leveled_inventory(id(LIST), 1, 1).unwrap();
    assert_eq!(
        inventory.stacks(id(A))[0].extras(),
        &[field(33, Field::Form(Some(id(FACTION))))]
    );
}

#[test]
fn npc_global_and_faction_rank_are_distinct_even_for_signed_rank_bits() {
    let mut world = metadata_world(vec![
        list(LIST, 0, 0, &[(1, WEAPON, 1)]).sub(T(b"COED"), coed(FACTION, (-2_i32) as u32, 1.0)),
    ]);
    let inventory = world.roll_leveled_inventory(id(LIST), 1, 1).unwrap();
    assert_eq!(
        inventory.stacks(id(WEAPON))[0].extra(35),
        Some(&field(35, Field::Word((-2_i32) as u32)))
    );
}

#[test]
fn malformed_coed_rejects_before_random_selection() {
    let invalid = [
        coed(0xdead, 0, 1.0),
        coed(WEAPON, 0, 1.0),
        coed(NPC, FACTION, 1.0),
        coed(NPC, 0, f32::NAN),
        coed(NPC, 0, f32::INFINITY),
        vec![0; 11],
    ];
    for bytes in invalid {
        let mut world = metadata_world(vec![
            list(LIST, 1, 0, &[(1, A, 1), (1, WEAPON, 1)]).sub(T(b"COED"), bytes),
        ]);
        let before = world.save_state();
        let expected_rng = world.random_u64();
        world.load_state(&before).unwrap();
        assert!(world.roll_leveled_inventory(id(LIST), 1, 1).is_err());
        assert_eq!(world.random_u64(), expected_rng);
    }
    for record in [
        list(LIST, 0, 0, &[]).sub(T(b"COED"), coed(NPC, 0, 1.0)),
        list(LIST, 0, 0, &[(1, WEAPON, 1)])
            .sub(T(b"COED"), coed(NPC, 0, 1.0))
            .sub(T(b"COED"), coed(FACTION, 1, 1.0)),
    ] {
        assert!(
            metadata_world(vec![record])
                .roll_leveled_inventory(id(LIST), 1, 1)
                .is_err()
        );
    }
}

#[test]
fn winning_override_resolves_owner_and_global_in_its_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(list(LIST, 0, 0, &[(1, WEAPON, 1)]));
    base.push_top(Record::new(T(b"WEAP"), WEAPON));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(
        list(LIST, 0, 0, &[(1, WEAPON, 1)]).sub(T(b"COED"), coed(0x0100_0300, 0x0100_0302, 1.5)),
    );
    patch.push_top(Record::new(T(b"NPC_"), 0x0100_0300));
    patch.push_top(Record::new(T(b"GLOB"), 0x0100_0302));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    let inventory = world.roll_leveled_inventory(id(LIST), 1, 1).unwrap();
    assert_eq!(
        inventory.stacks(id(WEAPON))[0].extra(33),
        Some(&field(
            33,
            Field::Form(Some(GlobalFormId {
                file: 1,
                object: NPC
            }))
        ))
    );
    assert_eq!(
        inventory.stacks(id(WEAPON))[0].extra(34),
        Some(&field(
            34,
            Field::Form(Some(GlobalFormId {
                file: 1,
                object: GLOBAL
            }))
        ))
    );
}

#[test]
fn script_added_entries_have_no_authored_metadata_after_save_load() {
    let records =
        || vec![list(LIST, 4, 0, &[(1, WEAPON, 2)]).sub(T(b"COED"), coed(NPC, GLOBAL, 1.5))];
    let mut world = metadata_world(records());
    world
        .leveled_list(id(LIST))
        .unwrap()
        .entries
        .push(soe_game::world::LeveledEntry {
            level: 1,
            form: id(WEAPON),
            count: 3,
        });
    let saved = world.save_state();
    let mut restored = metadata_world(records());
    restored.load_state(&saved).unwrap();
    let inventory = restored.roll_leveled_inventory(id(LIST), 1, 2).unwrap();
    assert_eq!(inventory.count(id(WEAPON)), 10);
    assert_eq!(inventory.stacks(id(WEAPON))[0].count(), 4);
}
