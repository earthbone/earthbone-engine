// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Distinct stacks must survive quantity operations and engine-save boundaries.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    inventory::{Extra, Field, Inventory, Stack},
    World,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn stack(count: i32) -> Stack {
    Stack::new(
        count,
        vec![
            Extra {
                kind: 22,
                fields: vec![],
            },
            Extra {
                kind: 37,
                fields: vec![Field::FloatBits(1.5_f32.to_bits())],
            },
            Extra {
                kind: 40,
                fields: vec![Field::FloatBits(0x7FC00042)],
            },
            Extra {
                kind: 33,
                fields: vec![Field::Form(Some(id(0x810)))],
            },
            Extra {
                kind: 155,
                fields: vec![Field::Form(Some(id(0x811))), Field::Short(500)],
            },
            Extra {
                kind: 153,
                fields: vec![
                    Field::Text(vec![0x81, 0xE9]),
                    Field::Array(vec![vec![Field::Group(vec![
                        Field::Word(17),
                        Field::Short(23),
                        Field::Byte(5),
                        Field::Bytes(vec![1, 0]),
                    ])]]),
                ],
            },
        ],
    )
    .unwrap()
}

fn world() -> World {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"CONT"), 0x800));
    p.push_top(Record::new(Tag(*b"WEAP"), 0x801));
    p.push_top(Record::new(Tag(*b"NPC_"), 0x810));
    p.push_top(Record::new(Tag(*b"ENCH"), 0x811));
    for object in [0x900, 0x901] {
        p.push_top(Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("Items.esm", p).unwrap();
    World::new(order)
}

#[test]
fn splitting_a_stack_preserves_extras_and_plain_items_are_consumed_first() {
    let item = id(0x801);
    let original = stack(3);
    let mut inv = Inventory::default();
    inv.restore(item, 5, vec![original.clone()]).unwrap();
    let taken = inv.take(item, 3);
    assert_eq!(taken.count(), 3);
    assert_eq!(taken.stacks().len(), 1);
    assert_eq!(taken.stacks()[0].count(), 1);
    assert_eq!(taken.stacks()[0].extras(), original.extras());
    assert_eq!(inv.count(item), 2);
    assert_eq!(inv.stacks(item)[0].count(), 2);
    inv.receive(item, taken).unwrap();
    assert_eq!(inv.count(item), 5);
    assert_eq!(inv.stacks(item).len(), 2);
    assert_eq!(inv.take(item, i32::MAX).count(), 5);
    assert!(inv.is_empty());
}

#[test]
fn invalid_quantities_and_duplicate_count_fields_do_not_replace_inventory() {
    let item = id(0x801);
    let mut inv = Inventory::default();
    inv.restore(item, 3, vec![stack(2)]).unwrap();
    let before = inv.clone();
    assert!(inv.restore(item, -1, vec![stack(2)]).is_err());
    assert!(inv.add(item, i32::MAX).is_err());
    assert!(inv.add(item, -1).is_err());
    assert_eq!(inv, before);
    assert!(Stack::new(0, vec![]).is_err());
    assert!(Stack::new(
        1,
        vec![Extra {
            kind: 36,
            fields: vec![Field::Short(1)]
        }]
    )
    .is_err());
    assert!(Stack::new(
        1,
        vec![
            Extra {
                kind: 22,
                fields: vec![]
            };
            2
        ]
    )
    .is_err());
}

#[test]
fn transfers_preserve_enchantment_state_but_do_not_equip_the_recipient() {
    let (source, target, item) = (id(0x900), id(0x901), id(0x801));
    let mut w = world();
    w.reference(source)
        .unwrap()
        .inventory
        .restore(item, 3, vec![stack(3)])
        .unwrap();
    assert_eq!(w.remove_item(source, item, 1, Some(source)), 0);
    assert_eq!(w.remove_item(source, item, 1, Some(id(0xBAD))), 0);
    w.add_item(target, item, i32::MAX);
    assert_eq!(w.remove_item(source, item, 1, Some(target)), 0);
    assert_eq!(w.item_count(source, item), 3);
    w.reference(target).unwrap().inventory.clear();
    assert_eq!(w.remove_item(source, item, 2, Some(target)), 2);
    let left = w.reference_ref(source).unwrap().inventory.stacks(item);
    let moved = w.reference_ref(target).unwrap().inventory.stacks(item);
    assert_eq!(left[0].count(), 1);
    assert!(left[0].worn());
    assert_eq!(moved[0].count(), 2);
    assert!(!moved[0].worn());
    for tag in [33, 37, 40, 153, 155] {
        assert_eq!(moved[0].extra(tag), left[0].extra(tag));
    }
}

#[test]
fn engine_save_retains_every_stack_field_and_failed_load_is_atomic() {
    let (holder, item) = (id(0x900), id(0x801));
    let mut w = world();
    w.reference(holder)
        .unwrap()
        .inventory
        .restore(item, 7, vec![stack(3)])
        .unwrap();
    let hash = w.state_hash();
    let bytes = w.save_state();
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 16);
    let mut loaded = world();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(
        loaded.reference_ref(holder).unwrap().inventory,
        w.reference_ref(holder).unwrap().inventory
    );
    let mut changed = world();
    let mut other = stack(3).extras().to_vec();
    other[1].fields = vec![Field::FloatBits(2.0_f32.to_bits())];
    changed
        .reference(holder)
        .unwrap()
        .inventory
        .restore(item, 7, vec![Stack::new(3, other).unwrap()])
        .unwrap();
    assert_ne!(changed.state_hash(), hash);
    assert!(loaded.load_state(&bytes[..bytes.len() - 1]).is_err());
    assert_eq!(loaded.state_hash(), hash);
}

#[test]
fn version_eleven_counts_load_as_plain_items() {
    let mut w = world();
    w.add_item(id(0x900), id(0x801), 17);
    let mut bytes = w.save_state();
    bytes.truncate(bytes.len() - 17);
    bytes[4..6].copy_from_slice(&11_u16.to_le_bytes());
    let mut loaded = world();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.item_count(id(0x900), id(0x801)), 17);
    assert!(loaded
        .reference_ref(id(0x900))
        .unwrap()
        .inventory
        .stacks(id(0x801))
        .is_empty());
    assert_eq!(loaded.state_hash(), w.state_hash());
}

#[test]
fn signed_leveled_bookkeeping_survives_saves_without_becoming_transferable_gold() {
    let (holder, item) = (id(0x900), id(0x801));
    let marker = Stack::new(
        -60,
        vec![Extra {
            kind: 46,
            fields: vec![Field::Word(0), Field::Byte(1)],
        }],
    )
    .unwrap();
    let mut w = world();
    w.reference(holder)
        .unwrap()
        .inventory
        .restore(item, 0, vec![marker.clone()])
        .unwrap();
    assert_eq!(w.remove_item(holder, item, 1, Some(id(0x901))), 0);
    let mut loaded = world();
    loaded.load_state(&w.save_state()).unwrap();
    assert_eq!(loaded.state_hash(), w.state_hash());
    assert_eq!(
        loaded.reference_ref(holder).unwrap().inventory.stacks(item),
        std::slice::from_ref(&marker)
    );
    loaded.add_item(holder, item, 3);
    assert_eq!(loaded.remove_item(holder, item, 3, Some(id(0x901))), 3);
    assert_eq!(loaded.item_count(holder, item), 0);
    assert_eq!(loaded.item_count(id(0x901), item), 3);
    assert_eq!(
        loaded.reference_ref(holder).unwrap().inventory.stacks(item),
        &[marker]
    );
    assert!(loaded
        .reference_ref(id(0x901))
        .unwrap()
        .inventory
        .stacks(item)
        .is_empty());
}

#[test]
fn overlapping_saved_lists_do_not_inflate_the_authoritative_item_total() {
    let item = id(0x801);
    let mut inventory = Inventory::default();
    inventory
        .restore(item, 2, vec![stack(1), stack(2)])
        .unwrap();
    assert_eq!(inventory.count(item), 2);
    let moved = inventory.take(item, 10);
    assert_eq!(moved.count(), 2);
    assert_eq!(inventory.count(item), 0);
    assert_eq!(inventory.take(item, 10).count(), 0);
    let mut recipient = Inventory::default();
    recipient.receive(item, moved).unwrap();
    assert_eq!(recipient.count(item), 2);
}
