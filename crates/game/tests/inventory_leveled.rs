// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AddItem transaction checks over selected LVLI inventory and metadata.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Pending, World, inventory::Field};
use soe_papyrus::{ObjectId, Value};

const ITEM: u32 = 0x800;
const CONTAINER: u32 = 0x801;
const HOLDER: u32 = 0x802;
const OTHER: u32 = 0x804;
const LIST: u32 = 0x900;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(flags: u8, chance: u8) -> World {
    world_with_levels(flags, chance, 1)
}

fn world_with_levels(flags: u8, chance: u8, other_level: u16) -> World {
    World::new(order(flags, chance, other_level, None))
}

fn order(flags: u8, chance: u8, other_level: u16, zone_level: Option<u8>) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for (tag, object) in [
        (T(b"WEAP"), ITEM),
        (T(b"CONT"), CONTAINER),
        (T(b"MISC"), OTHER),
    ] {
        plugin.push_top(Record::new(tag, object));
    }
    let mut holder = Record::new(T(b"REFR"), HOLDER).sub(T(b"NAME"), CONTAINER.to_le_bytes());
    if let Some(level) = zone_level {
        let mut zone = vec![0_u8; 12];
        zone[9] = level;
        zone[11] = level;
        plugin.push_top(Record::new(T(b"ECZN"), 0xA00).sub(T(b"DATA"), zone));
        holder = holder.sub(T(b"XEZN"), 0xA00_u32.to_le_bytes());
    }
    plugin.push_top(holder);
    let mut list = Record::new(T(b"LVLI"), LIST)
        .sub(T(b"LVLD"), [chance])
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LLCT"), [2]);
    for (item, count) in [(ITEM, 2_u16), (OTHER, 1)] {
        let mut entry = vec![0_u8; 12];
        let level = if item == OTHER { other_level } else { 1 };
        entry[..2].copy_from_slice(&level.to_le_bytes());
        entry[4..8].copy_from_slice(&item.to_le_bytes());
        entry[8..10].copy_from_slice(&count.to_le_bytes());
        list = list.sub(T(b"LVLO"), entry);
        if item == ITEM {
            list = list.sub(
                T(b"COED"),
                [
                    0_u32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    1.25_f32.to_le_bytes(),
                ]
                .concat(),
            );
        }
    }
    plugin.push_top(list);
    let mut nested = vec![0_u8; 12];
    nested[..2].copy_from_slice(&1_u16.to_le_bytes());
    nested[4..8].copy_from_slice(&LIST.to_le_bytes());
    nested[8..10].copy_from_slice(&1_u16.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"LVLI"), LIST + 1)
            .sub(T(b"LVLF"), [0])
            .sub(T(b"LLCT"), [1])
            .sub(T(b"LVLO"), nested),
    );
    let mut order = LoadOrder::new();
    order.add("Inventory.esm", plugin).unwrap();
    order
}

fn allocator() -> impl FnMut(&str, &str) -> ObjectId {
    let mut next = 0;
    move |_, _| {
        next += 1;
        ObjectId(next)
    }
}

#[test]
fn selected_items_preserve_metadata_and_emit_one_event_per_base() {
    let mut world = world(4, 0);
    assert!(world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, 3, &mut allocator()));
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 6);
    assert_eq!(world.item_count(id(HOLDER), id(OTHER)), 3);
    let stack = &world
        .reference_ref(id(HOLDER))
        .unwrap()
        .inventory
        .stacks(id(ITEM))[0];
    assert_eq!(stack.count(), 6);
    assert_eq!(
        stack.extra(37).unwrap().fields,
        vec![Field::FloatBits(1.25_f32.to_bits())]
    );
    let notices: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| {
            if let Pending::Event { name, args, .. } = pending {
                Some((
                    name.as_str(),
                    world.form_arg(&args[0]),
                    args[1].clone(),
                    args[2].clone(),
                ))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        notices,
        vec![
            ("OnItemAdded", Some(id(ITEM)), Value::Int(6), Value::None),
            ("OnItemAdded", Some(id(OTHER)), Value::Int(3), Value::None),
        ]
    );
    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 6);
    assert_eq!(
        world
            .reference_ref(id(HOLDER))
            .unwrap()
            .inventory
            .stacks(id(ITEM))[0]
            .extra(37)
            .unwrap()
            .fields,
        vec![Field::FloatBits(1.25_f32.to_bits())]
    );
}

#[test]
fn capacity_failure_preserves_all_items_events_and_rng() {
    let mut world = world(1, 0);
    world.add_item(id(HOLDER), id(ITEM), i32::MAX);
    world.add_item(id(HOLDER), id(OTHER), i32::MAX);
    let saved = world.save_state();
    let expected_rng = world.random_u64();
    world.load_state(&saved).unwrap();
    assert!(!world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, 1, &mut allocator()));
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), i32::MAX);
    assert_eq!(world.item_count(id(HOLDER), id(OTHER)), i32::MAX);
    assert!(world.pending.is_empty());
    assert_eq!(world.random_u64(), expected_rng);
}

#[test]
fn empty_selection_succeeds_without_events_and_nonpositive_counts_reject() {
    let mut world = world(4, 100);
    assert!(world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, 1, &mut allocator()));
    for count in [0, -1] {
        assert!(!world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, count, &mut allocator()));
    }
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 0);
    assert!(world.pending.is_empty());
}

#[test]
fn late_capacity_failure_discards_earlier_staged_items_and_metadata() {
    let mut world = world(4, 0);
    world.add_item(id(HOLDER), id(OTHER), i32::MAX);
    // Establish the lazy list view before comparing transaction state.
    world.leveled_list(id(LIST)).unwrap();
    let saved = world.save_state();
    assert!(!world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, 1, &mut allocator()));
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 0);
    assert_eq!(world.item_count(id(HOLDER), id(OTHER)), i32::MAX);
    assert!(
        world
            .reference_ref(id(HOLDER))
            .unwrap()
            .inventory
            .stacks(id(ITEM))
            .is_empty()
    );
    assert!(world.pending.is_empty());
    assert_eq!(world.save_state(), saved);
}

#[test]
fn add_item_caps_ordinary_selection_at_player_level() {
    let mut world = world_with_levels(0, 0, 10);
    world.player_level = 5;
    let selected = world.roll_leveled_inventory(id(LIST), 20, 1).unwrap();
    assert_eq!(selected.count(id(OTHER)), 1);
    assert!(world.add_leveled_item_notified(id(HOLDER), id(LIST), 20, 1, &mut allocator()));
    assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 2);
    assert_eq!(world.item_count(id(HOLDER), id(OTHER)), 0);
}

#[test]
fn add_item_special_loot_bypasses_player_cap_even_under_an_ordinary_parent() {
    for list in [LIST, LIST + 1] {
        let mut world = world_with_levels(8, 0, 10);
        world.player_level = 5;
        for bound in ["min", "max"] {
            for (part, value) in [
                ("pclevelmult", 0.0),
                ("pclevelbase", 0.0),
                ("zonelevelbase", 0.0),
                ("zonelevelmult", 1.0),
            ] {
                world
                    .settings
                    .insert(format!("fspecialloot{bound}{part}"), Value::Float(value));
            }
        }
        world
            .settings
            .insert("fspeciallootweighting".into(), Value::Float(1.0));
        assert!(world.add_leveled_item_notified(id(HOLDER), id(list), 20, 1, &mut allocator()));
        assert_eq!(world.item_count(id(HOLDER), id(OTHER)), 1);
        assert_eq!(world.item_count(id(HOLDER), id(ITEM)), 0);
    }
}

#[test]
fn special_mixed_flags_preserve_each_precedence_and_selected_metadata() {
    for flags in [13, 14, 15] {
        let mut world = world_with_levels(flags, 0, 10);
        world.player_level = 1;
        for bound in ["min", "max"] {
            for (part, value) in [
                ("pclevelmult", 0.0),
                ("pclevelbase", 10.0),
                ("zonelevelbase", 0.0),
                ("zonelevelmult", 0.0),
            ] {
                world
                    .settings
                    .insert(format!("fspecialloot{bound}{part}"), Value::Float(value));
            }
        }
        world
            .settings
            .insert("fspeciallootweighting".into(), Value::Float(1.0));
        assert!(world.add_leveled_item_notified(id(HOLDER), id(LIST), 1, 200, &mut allocator()));
        let item = world.item_count(id(HOLDER), id(ITEM));
        let other = world.item_count(id(HOLDER), id(OTHER));
        match flags {
            13 => assert_eq!((item, other), (400, 200)),
            14 => assert_eq!((item, other), (0, 200)),
            15 => {
                assert!(item > 0 && other > 0);
                assert_eq!(item / 2 + other, 200);
            }
            _ => unreachable!(),
        }
        if item > 0 {
            let stacks = world
                .reference_ref(id(HOLDER))
                .unwrap()
                .inventory
                .stacks(id(ITEM));
            assert_eq!(stacks.iter().map(|stack| stack.count()).sum::<i32>(), item);
            for stack in stacks {
                assert_eq!(
                    stack.extra(37).unwrap().fields,
                    vec![Field::FloatBits(1.25_f32.to_bits())]
                );
            }
        }
    }
}

#[test]
fn papyrus_add_item_uses_receiver_zone_and_preserves_count_semantics() {
    use soe_game::Game;
    use soe_papyrus::Opcode::{CallMethod, Return};
    use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, none, s};

    for (flags, zone, player, expected_item, expected_other) in
        [(0, 5, 20, 6, 0), (0, 20, 5, 6, 0), (8, 20, 1, 0, 3)]
    {
        let mut game = Game::new(order(flags, 0, 10, Some(zone)));
        {
            let mut world = game.world_mut();
            world.player_level = player;
            if flags == 8 {
                for bound in ["min", "max"] {
                    for (part, value) in [
                        ("pclevelmult", 0.0),
                        ("pclevelbase", 0.0),
                        ("zonelevelbase", 0.0),
                        ("zonelevelmult", 1.0),
                    ] {
                        world
                            .settings
                            .insert(format!("fspecialloot{bound}{part}"), Value::Float(value));
                    }
                }
                world
                    .settings
                    .insert("fspeciallootweighting".into(), Value::Float(1.0));
            }
        }
        let function = FunctionBuilder::new("Run", "None")
            .global()
            .param("holder", "ObjectReference")
            .param("item", "Form")
            .param("count", "Int")
            .ins(
                CallMethod,
                vec![
                    s("AddItem"),
                    ident("holder"),
                    none(),
                    ident("item"),
                    ident("count"),
                ],
            )
            .ins(Return, vec![none()])
            .build();
        game.load_script(
            &ScriptBuilder::new("LeveledProbe", "Form")
                .function(function)
                .bytes(),
        )
        .unwrap();
        let holder = Value::Object(game.object_of_form(id(HOLDER)));
        let list = Value::Object(game.object_of_form(id(LIST)));
        for count in [0, -1, 3] {
            game.vm
                .call_global(
                    "LeveledProbe",
                    "Run",
                    &[holder.clone(), list.clone(), Value::Int(count)],
                )
                .unwrap();
            game.run_until_idle(64);
            let mut world = game.world_mut();
            assert_eq!(
                world.item_count(id(HOLDER), id(ITEM)),
                if count > 0 { expected_item } else { 0 }
            );
            assert_eq!(
                world.item_count(id(HOLDER), id(OTHER)),
                if count > 0 { expected_other } else { 0 }
            );
        }
    }
}
