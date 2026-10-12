// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Reset after SOEW loading, following CK Reset and UESP LVLI chance-none.

use std::{collections::BTreeMap, sync::Arc};

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const CELL: u32 = 0x100;
const OTHER_CELL: u32 = 0x101;
const FIXED: u32 = 0x300;
const LOOT: u32 = 0x301;
const ADDED: u32 = 0x302;
const LIST: u32 = 0x303;
const BASE: u32 = 0x400;
const REFERENCE: u32 = 0x500;
const TARGET: u32 = 0x501;
const ACTOR: u32 = 0x600;
const NPC: u32 = 0x700;
const FACTION: u32 = 0x701;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placement(object: u32, base: u32, cell: u32, position: [f32; 3]) -> Record {
    let mut record = Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            position
                .into_iter()
                .chain([0.25_f32, 0.5, 0.75])
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    record.placement.cell = Some(cell);
    record
}

fn fixture_order(leveled: bool) -> Arc<LoadOrder> {
    let mut master = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..FileHeader::default()
    });
    for cell in [CELL, OTHER_CELL] {
        master.push_top(Record::new(Tag(*b"CELL"), cell));
    }
    for item in [FIXED, LOOT, ADDED] {
        master.push_top(Record::new(Tag(*b"MISC"), item));
    }
    master.push_top(
        Record::new(Tag(*b"LVLI"), LIST)
            .sub(Tag(*b"LVLD"), [50])
            .sub(Tag(*b"LVLF"), [0])
            .sub(Tag(*b"LLCT"), [1])
            .sub(
                Tag(*b"LVLO"),
                [1_u32.to_le_bytes(), LOOT.to_le_bytes(), 4_u32.to_le_bytes()].concat(),
            ),
    );
    master.push_top(Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [FIXED.to_le_bytes(), 2_i32.to_le_bytes()].concat(),
    ));
    master.push_top(placement(REFERENCE, BASE, CELL, [0.0; 3]));
    master.push_top(placement(TARGET, BASE, OTHER_CELL, [100.0, 200.0, 300.0]));
    master.push_top(Record::new(Tag(*b"NPC_"), NPC));
    master.push_top(Record::new(Tag(*b"FACT"), FACTION));
    let mut actor = placement(ACTOR, NPC, CELL, [5.0, 6.0, 7.0]);
    actor.tag = Tag(*b"ACHR");
    master.push_top(actor);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Reset.esm".into()],
        ..FileHeader::default()
    });
    let mut container = Record::new(Tag(*b"CONT"), BASE).sub(
        Tag(*b"CNTO"),
        [FIXED.to_le_bytes(), 7_i32.to_le_bytes()].concat(),
    );
    if leveled {
        container = container.sub(
            Tag(*b"CNTO"),
            [LIST.to_le_bytes(), 1_i32.to_le_bytes()].concat(),
        );
    }
    patch.push_top(container);
    patch.push_top(
        placement(REFERENCE, BASE, CELL, [10.0, 20.0, 30.0])
            .sub(Tag(*b"XSCL"), 1.25_f32.to_le_bytes())
            .with_flags(flags::INITIALLY_DISABLED),
    );
    let mut order = LoadOrder::new();
    order.add("Reset.esm", master).unwrap();
    order.add("ResetPatch.esp", patch).unwrap();
    Arc::new(order)
}

#[test]
fn sav_71_fixed_equality_fixtures_follow_winning_container_contents() {
    let fixed = fixture_order(false);
    assert_eq!(
        crate::common::installed::fixed_container_references(&fixed).collect::<Vec<_>>(),
        vec![id(REFERENCE), id(TARGET)],
        "fixed fixtures use the winning base override in its source namespace"
    );
    let leveled = fixture_order(true);
    assert_eq!(
        crate::common::installed::fixed_container_references(&leveled).count(),
        0,
        "a winning base with LVLI contents cannot compare independent reset draws"
    );
}

#[test]
fn sav_71_loaded_reset_restores_winning_record_and_clears_runtime_changes() {
    let order = fixture_order(false);
    let mut world = World::from_shared_order(order.clone());
    let authored = world.reference(id(REFERENCE)).unwrap().clone();
    assert_eq!(authored.inventory.count(id(FIXED)), 7);
    let changed = world.reference(id(REFERENCE)).unwrap();
    changed.position = [1.0; 3];
    changed.rotation = [0.0; 3];
    changed.scale = 9.0;
    changed.cell = Some(id(OTHER_CELL));
    changed.enabled = true;
    changed.opened = true;
    changed.locked = true;
    changed.lock_level = 99;
    changed.activation_blocked = true;
    changed.deleted = true;
    changed.destroyed = true;
    changed.owner = Some(id(FACTION));
    changed.display_name = Some("Changed".into());
    changed.anim_ints.insert("stage".into(), 3);
    changed.inventory.take(id(FIXED), 7);
    changed.inventory.add(id(ADDED), 99).unwrap();
    let saved = world.save_state();
    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.save_state(), saved);
    assert!(loaded.reset_reference(id(REFERENCE), None));
    assert_eq!(loaded.reference_ref(id(REFERENCE)), Some(&authored));
    let reset = loaded.save_state();
    let mut again = World::from_shared_order(order);
    again.load_state(&reset).unwrap();
    assert_eq!(again.save_state(), reset);
    assert_eq!(again.reference_ref(id(REFERENCE)), Some(&authored));
}

#[test]
fn sav_71_loaded_reset_reselects_leveled_contents_and_reload_retains_the_draw() {
    let order = fixture_order(true);
    let mut world = World::from_shared_order(order.clone());
    let mut empty = false;
    let mut populated = false;
    for _ in 0..32 {
        let changed = world.reference(id(REFERENCE)).unwrap();
        changed.inventory.add(id(ADDED), 99).unwrap();
        let mut loaded = World::from_shared_order(order.clone());
        loaded.load_state(&world.save_state()).unwrap();
        assert!(loaded.reset_reference(id(REFERENCE), None));
        let inventory = &loaded.reference_ref(id(REFERENCE)).unwrap().inventory;
        assert_eq!(inventory.count(id(FIXED)), 7);
        assert_eq!(inventory.count(id(ADDED)), 0);
        assert_eq!(inventory.count(id(LIST)), 0);
        match inventory.count(id(LOOT)) {
            0 => empty = true,
            4 => populated = true,
            count => panic!("unexpected leveled count {count}"),
        }
        let reset = loaded.reference_ref(id(REFERENCE)).unwrap().clone();
        let saved = loaded.save_state();
        loaded.reference(id(REFERENCE)).unwrap();
        assert_eq!(loaded.save_state(), saved, "lookup must retain the draw");
        world = World::from_shared_order(order.clone());
        world.load_state(&saved).unwrap();
        assert_eq!(
            world.save_state(),
            saved,
            "load must retain contents and RNG"
        );
        assert_eq!(world.reference_ref(id(REFERENCE)), Some(&reset));
    }
    assert!(
        empty && populated,
        "the fixed seed must exercise both outcomes"
    );
}

#[test]
fn sav_71_loaded_reset_uses_optional_target_only_with_resident_source_cell() {
    let order = fixture_order(false);
    let mut world = World::from_shared_order(order.clone());
    world.reference(id(REFERENCE)).unwrap().position = [1.0; 3];
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(id(REFERENCE), Some(id(TARGET))));
    assert_eq!(
        loaded.reference_ref(id(REFERENCE)).unwrap().position,
        [10.0, 20.0, 30.0]
    );
    loaded
        .update_streaming_cells(id(CELL), &[id(CELL), id(OTHER_CELL)], &[])
        .unwrap();
    assert!(loaded.reset_reference(id(REFERENCE), Some(id(TARGET))));
    let reset = loaded.reference_ref(id(REFERENCE)).unwrap();
    assert_eq!(reset.position, [100.0, 200.0, 300.0]);
    assert_eq!(reset.cell, Some(id(OTHER_CELL)));
    assert_eq!(reset.rotation, [0.25, 0.5, 0.75]);
}

#[test]
fn sav_71_loaded_actor_reset_resurrects_and_retains_factions_and_relationships() {
    let order = fixture_order(false);
    let mut world = World::from_shared_order(order.clone());
    let actor = world.actor(id(ACTOR)).unwrap();
    actor.dead = true;
    actor.factions.insert(id(FACTION), 2);
    actor.relationships.insert(id(TARGET), 3);
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    assert!(loaded.reset_reference(id(ACTOR), None));
    let actor = loaded.actor(id(ACTOR)).unwrap();
    assert!(!actor.dead);
    assert_eq!(actor.factions.get(&id(FACTION)), Some(&2));
    assert_eq!(actor.relationships.get(&id(TARGET)), Some(&3));
}

#[test]
fn sav_71_failed_cyclic_inventory_reset_preserves_loaded_reference() {
    let order = fixture_order(true);
    let mut world = World::from_shared_order(order.clone());
    world
        .reference(id(REFERENCE))
        .unwrap()
        .inventory
        .add(id(ADDED), 99)
        .unwrap();
    world.leveled_list(id(LIST)).unwrap().entries[0].form = id(LIST);
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    let before = loaded.save_state();
    assert!(!loaded.reset_reference(id(REFERENCE), None));
    assert_eq!(
        loaded.save_state(),
        before,
        "failed reset must preserve saved state"
    );
}

#[test]
fn sav_71_installed_changed_fixed_and_leveled_containers_reset_after_load() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping SAV-71 installed container survey");
        return;
    };
    let fixed: Vec<_> = crate::common::installed::fixed_container_references(&order)
        .take(32)
        .collect();
    let leveled: Vec<_> = order
        .winners_of(Tag(*b"REFR"))
        .filter_map(|(reference, record)| {
            let raw = u32::from_le_bytes(record.find(Tag(*b"NAME"))?.try_into().ok()?);
            let base = order.resolve(order.winner_ref(reference)?.file, raw);
            let base_ref = order.winner_ref(base)?;
            let container = order.record(base_ref);
            (!record.is_deleted()
                && !container.is_deleted()
                && container.tag == Tag(*b"CONT")
                && container.all(Tag(*b"CNTO")).any(|cnto| {
                    cnto.get(..4)
                        .and_then(|bytes| bytes.try_into().ok())
                        .map(u32::from_le_bytes)
                        .and_then(|raw| order.winner(order.resolve(base_ref.file, raw)))
                        .is_some_and(|item| item.tag == Tag(*b"LVLI"))
                }))
            .then_some(reference)
        })
        .take(32)
        .collect();
    assert_eq!(fixed.len(), 32, "fixed inventory survey size");
    assert_eq!(leveled.len(), 32, "leveled inventory survey size");
    let mut world = World::from_shared_order(order.clone());
    let gold = world.form("Gold001").unwrap();
    let mut authored = BTreeMap::new();
    for reference in fixed.iter().chain(&leveled) {
        let state = world.reference(*reference).unwrap();
        authored.insert(*reference, state.clone());
        state.position[0] += 256.0;
        state.enabled = !state.enabled;
        state.opened = true;
        state.inventory.add(gold, 1_000_000).unwrap();
    }
    let changed = world.save_state();
    let mut loaded = World::from_shared_order(order.clone());
    loaded.load_state(&changed).unwrap();
    assert_eq!(loaded.save_state(), changed);
    for reference in fixed.iter().chain(&leveled) {
        assert!(
            loaded.reset_reference(*reference, None),
            "reset {reference}"
        );
        let reset = loaded.reference_ref(*reference).unwrap();
        let original = &authored[reference];
        assert_eq!(reset.position, original.position, "position {reference}");
        assert_eq!(reset.rotation, original.rotation, "rotation {reference}");
        assert_eq!(reset.enabled, original.enabled, "enable state {reference}");
        if fixed.contains(reference) {
            assert_eq!(
                reset.inventory, original.inventory,
                "fixed inventory {reference}"
            );
        } else {
            assert!(
                reset.inventory.count(gold) < 1_000_000,
                "added gold cleared {reference}"
            );
            assert!(
                reset
                    .inventory
                    .iter()
                    .all(|(item, _)| loaded.tag_of(*item) != Some(Tag(*b"LVLI"))),
                "concrete selection {reference}"
            );
        }
    }
    let saved = loaded.save_state();
    let mut again = World::from_shared_order(order);
    again.load_state(&saved).unwrap();
    assert_eq!(
        again.save_state(),
        saved,
        "reset inventory and RNG round trip"
    );
    for reference in authored.keys() {
        assert_eq!(
            again.reference_ref(*reference),
            loaded.reference_ref(*reference)
        );
    }
    eprintln!("SAV-71 reset after load: 32 fixed and 32 leveled containers across five masters");
}
