// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Default outfits produce their own once-only, worn item instances.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    World,
    inventory::{Extra, Field, Stack},
    npc,
    world::RngStream,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;
const ACTOR: u32 = 0x200;
const OUTFIT: u32 = 0x300;
const A: u32 = 0x400;
const B: u32 = 0x401;
const LIST: u32 = 0x500;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn armor(object: u32, slots: u32) -> Record {
    Record::new(T(b"ARMO"), object)
        .sub(
            T(b"DATA"),
            [0_u32.to_le_bytes(), 1_f32.to_le_bytes()].concat(),
        )
        .sub(T(b"DNAM"), 100_u32.to_le_bytes())
        .sub(
            T(b"BOD2"),
            [slots.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
        )
}
fn actor_base() -> Record {
    npc::build_record(
        BASE,
        "Actor",
        "Actor",
        npc::Config {
            level: 20,
            ..Default::default()
        },
        npc::Stats::default(),
    )
    .sub(T(b"DOFT"), OUTFIT.to_le_bytes())
}
fn outfit(object: u32, entries: &[u32]) -> Record {
    Record::new(T(b"OTFT"), object).sub(
        T(b"INAM"),
        entries
            .iter()
            .flat_map(|item| item.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}
fn list(object: u32, flags: u8, entries: &[(u16, u32, u16)]) -> Record {
    let mut record = Record::new(T(b"LVLI"), object).sub(T(b"LVLF"), [flags]);
    for (level, item, count) in entries {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&item.to_le_bytes());
        bytes[8..10].copy_from_slice(&count.to_le_bytes());
        record = record.sub(T(b"LVLO"), bytes);
    }
    record
}
fn fixture(extra: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        actor_base(),
        Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), BASE.to_le_bytes()),
        armor(A, 1 << 2),
        armor(B, 1 << 7),
    ] {
        plugin.push_top(record);
    }
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}
fn worn(world: &World, item: u32) -> usize {
    world
        .reference_ref(id(ACTOR))
        .unwrap()
        .inventory
        .stacks(id(item))
        .iter()
        .filter(|stack| stack.worn())
        .count()
}

#[test]
fn inv_03_default_outfit_adds_and_equips_its_own_instance_without_events() {
    let base = actor_base()
        .sub(T(b"CNTO"), [A.to_le_bytes(), 3_i32.to_le_bytes()].concat())
        .sub(T(b"SOFT"), (OUTFIT + 1).to_le_bytes());
    let mut world = fixture(vec![base, outfit(OUTFIT, &[A]), outfit(OUTFIT + 1, &[B])]);
    let state = world.reference(id(ACTOR)).unwrap();
    assert_eq!(state.inventory.count(id(A)), 4);
    assert_eq!(state.inventory.count(id(B)), 0);
    let stack = &state.inventory.stacks(id(A))[0];
    assert_eq!(stack.count(), 1);
    assert!(stack.worn());
    assert_eq!(
        stack.extra(142).unwrap().fields,
        vec![Field::Form(Some(id(OUTFIT)))]
    );
    assert!(world.actor_ref(id(ACTOR)).is_none());
    assert_eq!(
        world
            .actor(id(ACTOR))
            .unwrap()
            .equipped
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![id(A)]
    );
    assert!(world.pending.is_empty());
}

#[test]
fn outfit_lists_use_npc_level_add_one_per_base_and_discard_coed() {
    let coed = list(LIST, 0, &[(1, A, 3), (20, B, 17)]).sub(T(b"COED"), [1, 2, 3]);
    let mut world = fixture(vec![outfit(OUTFIT, &[LIST]), coed]);
    world.player_level = 1;
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(world.item_count(id(ACTOR), id(A)), 0);
    assert_eq!(world.item_count(id(ACTOR), id(B)), 1);
    assert_eq!(worn(&world, B), 1);
    assert_eq!(
        world
            .reference_ref(id(ACTOR))
            .unwrap()
            .inventory
            .stacks(id(B))[0]
            .extras()
            .iter()
            .map(|extra| extra.kind)
            .collect::<Vec<_>>(),
        vec![142, 22]
    );
    assert!(world.roll_leveled_inventory(id(LIST), 20, 1).is_err());
}

#[test]
fn duplicate_entries_and_nested_counts_produce_one_outfit_tagged_copy() {
    let mut world = fixture(vec![
        outfit(OUTFIT, &[A, A, LIST]),
        list(LIST, 5, &[(99, LIST + 1, 9), (99, A, 8)]),
        list(LIST + 1, 4, &[(1, A, 7), (1, B, 11)]),
    ]);
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(world.item_count(id(ACTOR), id(A)), 1);
    assert_eq!(world.item_count(id(ACTOR), id(B)), 1);
    assert_eq!(worn(&world, A), 1);
    assert_eq!(worn(&world, B), 1);
}

#[test]
fn repeated_list_entries_roll_separately_before_deduplication() {
    let records = vec![
        outfit(OUTFIT, &[LIST, LIST]),
        list(LIST, 1, &[(1, A, 1), (1, B, 1)]),
    ];
    let mut expected = fixture(records.clone());
    expected.roll_leveled_items(id(LIST), 20, 1).unwrap();
    expected.roll_leveled_items(id(LIST), 20, 1).unwrap();
    let expected_rng = expected.random_u64_for(RngStream::LeveledItems);
    let mut world = fixture(records);
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(world.random_u64_for(RngStream::LeveledItems), expected_rng);
}

#[test]
fn conflicting_armor_replaces_worn_instance_but_retains_both_items() {
    let mut world = fixture(vec![outfit(OUTFIT, &[A, B]), armor(B, 1 << 2)]);
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(worn(&world, A), 0);
    assert_eq!(worn(&world, B), 1);
    assert_eq!(world.item_count(id(ACTOR), id(A)), 1);
    assert_eq!(
        world
            .actor(id(ACTOR))
            .unwrap()
            .equipped
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![id(B)]
    );
    world.equip_item(id(ACTOR), id(A)).unwrap();
    assert_eq!(worn(&world, B), 0);
}

#[test]
fn selected_inventory_template_supplies_outfit_and_armor_template_supplies_fields() {
    let mut base = actor_base().sub(T(b"TPLT"), (BASE + 1).to_le_bytes());
    let acbs = &mut base
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"ACBS"))
        .unwrap()
        .data;
    acbs[18..20].copy_from_slice(&npc::template::INVENTORY.to_le_bytes());
    let parent = npc::build_record(
        BASE + 1,
        "Parent",
        "Parent",
        npc::Config::default(),
        npc::Stats::default(),
    )
    .sub(T(b"DOFT"), (OUTFIT + 1).to_le_bytes());
    let child = armor(B, 1 << 7).sub(T(b"TNAM"), A.to_le_bytes());
    let mut world = fixture(vec![
        base,
        parent,
        outfit(OUTFIT, &[A]),
        outfit(OUTFIT + 1, &[B]),
        child,
    ]);
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(world.item_count(id(ACTOR), id(A)), 0);
    assert_eq!(worn(&world, B), 1);
    assert_eq!(world.actor(id(ACTOR)).unwrap().outfit, Some(id(OUTFIT + 1)));
}

#[test]
fn winning_outfit_override_uses_its_own_form_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(actor_base());
    base.push_top(Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), BASE.to_le_bytes()));
    base.push_top(outfit(OUTFIT, &[]));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(outfit(OUTFIT, &[0x0100_0400]));
    patch.push_top(armor(0x0100_0400, 1 << 2));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    let state = world.reference(id(ACTOR)).unwrap();
    assert_eq!(
        state.inventory.count(GlobalFormId { file: 1, object: A }),
        1
    );
    assert_eq!(state.inventory.count(id(A)), 0);
}

#[test]
fn inherited_outfit_uses_the_winning_template_namespace() {
    let child = npc::build_record(
        BASE,
        "Child",
        "Child",
        npc::Config {
            template_flags: npc::template::INVENTORY,
            ..Default::default()
        },
        npc::Stats::default(),
    )
    .sub(T(b"TPLT"), (BASE + 1).to_le_bytes());
    let parent = npc::build_record(
        BASE + 1,
        "Parent",
        "Parent",
        npc::Config::default(),
        npc::Stats::default(),
    );
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(child);
    base.push_top(parent.clone());
    base.push_top(Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), BASE.to_le_bytes()));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(parent.sub(T(b"DOFT"), 0x0100_0300_u32.to_le_bytes()));
    patch.push_top(outfit(0x0100_0300, &[0x0100_0400]));
    patch.push_top(armor(0x0100_0400, 1 << 2));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    let item = GlobalFormId { file: 1, object: A };
    assert_eq!(world.reference(id(ACTOR)).unwrap().inventory.count(item), 1);
    let actor = world.actor(id(ACTOR)).unwrap();
    assert_eq!(
        actor.outfit,
        Some(GlobalFormId {
            file: 1,
            object: OUTFIT
        })
    );
    assert!(actor.equipped.contains(&item));
}

#[test]
fn once_only_initialization_and_removed_outfit_survive_save_load() {
    let records = vec![
        outfit(OUTFIT, &[LIST]),
        list(LIST, 1, &[(1, A, 1), (1, B, 1)]),
    ];
    let mut world = fixture(records.clone());
    world.reference(id(ACTOR)).unwrap();
    world.actor(id(ACTOR)).unwrap();
    let bytes = world.save_state();
    let hash = world.state_hash();
    world.reference(id(ACTOR)).unwrap();
    world.actor(id(ACTOR)).unwrap();
    assert_eq!(world.save_state(), bytes);
    let mut loaded = fixture(records.clone());
    loaded.load_state(&bytes).unwrap();
    loaded.reference(id(ACTOR)).unwrap();
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(loaded.save_state(), bytes);
    loaded.reference(id(ACTOR)).unwrap().inventory.clear();
    loaded.actor(id(ACTOR)).unwrap().equipped.clear();
    let empty = loaded.save_state();
    let mut restored = fixture(records);
    restored.load_state(&empty).unwrap();
    assert!(restored.reference(id(ACTOR)).unwrap().inventory.is_empty());
    assert_eq!(restored.save_state(), empty);
}

#[test]
fn dynamic_actors_get_outfits_without_eager_actor_state_or_events() {
    let mut world = fixture(vec![outfit(OUTFIT, &[A])]);
    let dynamic = world.place_at(id(BASE), id(ACTOR), false);
    assert_eq!(
        world.reference_ref(dynamic).unwrap().inventory.count(id(A)),
        1
    );
    assert!(world.actor_ref(dynamic).is_none());
    assert!(world.actor(dynamic).unwrap().equipped.contains(&id(A)));
    assert!(world.pending.is_empty());
}

#[test]
fn restored_inventory_is_preserved_even_if_authored_outfit_now_differs() {
    let mut old = fixture(vec![outfit(OUTFIT, &[])]);
    old.reference(id(ACTOR))
        .unwrap()
        .inventory
        .restore(
            id(A),
            2,
            vec![
                Stack::new(
                    1,
                    vec![Extra {
                        kind: 33,
                        fields: vec![Field::Form(Some(id(BASE)))],
                    }],
                )
                .unwrap(),
            ],
        )
        .unwrap();
    let bytes = old.save_state();
    let mut loaded = fixture(vec![outfit(OUTFIT, &[B])]);
    loaded.load_state(&bytes).unwrap();
    let before = loaded.save_state();
    loaded.actor(id(ACTOR)).unwrap();
    assert_eq!(loaded.item_count(id(ACTOR), id(A)), 2);
    assert_eq!(loaded.item_count(id(ACTOR), id(B)), 0);
    assert!(loaded.actor(id(ACTOR)).unwrap().equipped.is_empty());
    assert_eq!(
        loaded.reference_ref(id(ACTOR)).unwrap().inventory,
        old.reference_ref(id(ACTOR)).unwrap().inventory
    );
    assert!(!before.is_empty());
}

#[test]
fn nonwearable_list_results_are_carried_without_worn_markers() {
    let mut world = fixture(vec![
        outfit(OUTFIT, &[LIST]),
        Record::new(T(b"MISC"), B),
        list(LIST, 4, &[(1, A, 1), (1, B, 8)]),
    ]);
    world.reference(id(ACTOR)).unwrap();
    assert_eq!(world.item_count(id(ACTOR), id(B)), 1);
    assert_eq!(worn(&world, B), 0);
    assert_eq!(worn(&world, A), 1);
}

#[test]
fn invalid_late_outfit_preserves_rng_and_does_not_install_partial_inventory() {
    let mut world = fixture(vec![
        outfit(OUTFIT, &[LIST, B]),
        list(LIST, 1, &[(1, A, 1), (1, B, 1)]),
        armor(B, 1 << 7).sub(T(b"TNAM"), (B + 1).to_le_bytes()),
    ]);
    let rng = world.state_hash().rng;
    assert!(world.reference(id(ACTOR)).is_none());
    assert!(world.reference_ref(id(ACTOR)).is_none());
    assert_eq!(world.state_hash().rng, rng);
    assert!(world.pending.is_empty());
}

#[test]
fn declared_weapon_ammunition_and_carryable_light_results_are_worn() {
    for (tag, carry, expected) in [
        (T(b"WEAP"), false, true),
        (T(b"AMMO"), false, true),
        (T(b"LIGH"), true, true),
        (T(b"LIGH"), false, false),
        (T(b"BOOK"), false, false),
    ] {
        let mut item = Record::new(tag, B);
        if tag == T(b"LIGH") {
            let mut data = vec![0; 48];
            data[12..16].copy_from_slice(&(if carry { 2_u32 } else { 0 }).to_le_bytes());
            item = item.sub(T(b"DATA"), data);
        }
        let mut world = fixture(vec![
            outfit(OUTFIT, &[LIST]),
            list(LIST, 0, &[(1, B, 5)]),
            item,
        ]);
        world.reference(id(ACTOR)).unwrap();
        assert_eq!(world.item_count(id(ACTOR), id(B)), 1);
        assert_eq!(worn(&world, B), usize::from(expected));
        assert_eq!(
            world.actor(id(ACTOR)).unwrap().equipped.contains(&id(B)),
            expected
        );
    }
}
