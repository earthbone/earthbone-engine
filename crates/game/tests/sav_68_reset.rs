// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-68 ObjectReference.Reset behavior and save persistence.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::{Game, World};
use soe_papyrus::{
    Opcode::CallMethod,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x100;
const TARGET_CELL: u32 = 0x101;
const REF: u32 = 0x200;
const TARGET: u32 = 0x201;
const BASE: u32 = 0x300;
const ITEM: u32 = 0x301;
const ACTOR: u32 = 0x202;
const ACTOR_BASE: u32 = 0x302;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn transform(position: [f32; 3]) -> Vec<u8> {
    position
        .into_iter()
        .chain([0.25_f32, 0.5, 0.75])
        .flat_map(f32::to_le_bytes)
        .collect()
}

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..FileHeader::default()
    });
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"CELL"), TARGET_CELL));
    plugin.push_top(
        Record::new(T(b"CONT"), BASE).sub(
            T(b"CNTO"),
            [
                ITEM.to_le_bytes().as_slice(),
                3_i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
    );
    plugin.push_top(Record::new(T(b"MISC"), ITEM));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
    let mut reference = Record::new(T(b"REFR"), REF)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(T(b"DATA"), transform([10.0, 20.0, 30.0]))
        .sub(T(b"XSCL"), 1.25_f32.to_le_bytes());
    reference.placement.cell = Some(CELL);
    reference.flags = flags::INITIALLY_DISABLED;
    plugin.push_top(reference);
    let mut target = Record::new(T(b"REFR"), TARGET)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(T(b"DATA"), transform([100.0, 200.0, 300.0]));
    target.placement.cell = Some(TARGET_CELL);
    plugin.push_top(target);
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), transform([5.0, 6.0, 7.0])),
    );
    let mut order = LoadOrder::new();
    order.add("ResetFixture.esm", plugin).unwrap();
    order
}

fn fixture_world() -> World {
    World::new(fixture_order())
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

#[test]
fn sav_68_reset_restores_editor_transform_enable_and_base_inventory() {
    let mut world = fixture_world();
    let reference = id(REF);
    let item = id(ITEM);
    let state = world.reference(reference).unwrap();
    assert_eq!(state.position, [10.0, 20.0, 30.0]);
    assert!(!state.enabled);
    assert_eq!(state.inventory.count(item), 3);

    let state = world.reference(reference).unwrap();
    state.position = [1.0, 2.0, 3.0];
    state.rotation = [0.0; 3];
    state.scale = 9.0;
    state.enabled = true;
    state.inventory.take(item, 3);
    assert!(world.reset_reference(reference, None));

    let state = world.reference_ref(reference).unwrap();
    assert_eq!(state.position, [10.0, 20.0, 30.0]);
    assert_eq!(state.rotation, [0.25, 0.5, 0.75]);
    assert_eq!(state.scale, 1.25);
    assert!(!state.enabled);
    assert_eq!(state.inventory.count(item), 3);
}

#[test]
fn sav_68_reset_clears_runtime_reference_changes() {
    let mut world = fixture_world();
    let reference = id(REF);
    let state = world.reference(reference).unwrap();
    state.activation_blocked = true;
    state.opened = true;
    state.locked = true;
    state.lock_level = 99;
    state.display_name = Some("runtime name".into());
    state.deleted = true;
    state.owner = Some(id(ITEM));
    state.on_map = true;
    state.fast_travel = true;
    state.destroyed = true;
    state.ignore_friendly_hits = true;
    state.alpha = 0.2;
    state.anim_floats.insert("speed".into(), 4.0);
    state.anim_ints.insert("stage".into(), 5);
    state.anim_bools.insert("active".into(), true);

    assert!(world.reset_reference(reference, None));
    let state = world.reference_ref(reference).unwrap();
    assert!(!state.activation_blocked);
    assert!(!state.opened);
    assert!(!state.locked);
    assert_eq!(state.lock_level, 0);
    assert_eq!(state.display_name, None);
    assert!(!state.deleted);
    assert_eq!(state.owner, None);
    assert!(!state.on_map);
    assert!(!state.fast_travel);
    assert!(!state.destroyed);
    assert!(!state.ignore_friendly_hits);
    assert_eq!(state.alpha, 1.0);
    assert!(state.anim_floats.is_empty());
    assert!(state.anim_ints.is_empty());
    assert!(state.anim_bools.is_empty());
}

#[test]
fn sav_68_reset_papyrus_native_resets_reference() {
    let mut game = Game::new(fixture_order());
    game.world_mut().reference(id(REF)).unwrap().opened = true;
    let object = game.object_of_form(id(REF));
    let script_name = format!("Sav68Probe{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let script = ScriptBuilder::new(&script_name, "ObjectReference")
        .function(
            FunctionBuilder::new("Run", "None")
                .local("result", "None")
                .ins(CallMethod, vec![s("Reset"), ident("self"), ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game.vm.attach(object, &script_name).unwrap();
    game.vm.call(object, "Run", &[]).unwrap();
    game.run_until_idle(32);
    assert!(!game.world().reference_ref(id(REF)).unwrap().opened);
}

#[test]
fn sav_68_reset_moves_to_loaded_optional_target() {
    let mut world = fixture_world();
    world
        .update_streaming_cells(id(CELL), &[id(CELL), id(TARGET_CELL)], &[])
        .unwrap();
    assert!(world.reset_reference(id(REF), Some(id(TARGET))));
    let state = world.reference_ref(id(REF)).unwrap();
    assert_eq!(state.position, [100.0, 200.0, 300.0]);
    assert_eq!(state.cell, Some(id(TARGET_CELL)));
    assert_eq!(state.rotation, [0.25, 0.5, 0.75]);
}

#[test]
fn sav_68_reset_ignores_optional_target_when_reference_cell_is_unloaded() {
    let mut world = fixture_world();
    world.reference(id(TARGET)).unwrap();
    assert!(world.reset_reference(id(REF), Some(id(TARGET))));
    let state = world.reference_ref(id(REF)).unwrap();
    assert_eq!(state.position, [10.0, 20.0, 30.0]);
    assert_eq!(state.cell, Some(id(CELL)));
}

#[test]
fn sav_68_reset_resurrects_actor_and_preserves_faction_and_relationship_data() {
    let mut world = fixture_world();
    let actor_id = id(ACTOR);
    let faction = id(ITEM);
    {
        let actor = world.actor(actor_id).unwrap();
        actor.dead = true;
        actor.factions.insert(faction, 2);
        actor.relationships.insert(id(REF), 3);
    }
    assert!(world.reset_reference(actor_id, None));
    let actor = world.actor(actor_id).unwrap();
    assert!(!actor.dead);
    assert_eq!(actor.factions.get(&faction), Some(&2));
    assert_eq!(actor.relationships.get(&id(REF)), Some(&3));
}

#[test]
fn sav_68_reset_state_survives_world_save_round_trip() {
    let mut world = fixture_world();
    world.reference(id(REF)).unwrap().opened = true;
    assert!(world.reset_reference(id(REF), None));
    let saved = world.save_state();

    let mut restored = World::new(fixture_order());
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.save_state(), saved);
    let state = restored.reference_ref(id(REF)).unwrap();
    assert!(!state.opened);
    assert_eq!(state.position, [10.0, 20.0, 30.0]);
    assert_eq!(state.inventory.count(id(ITEM)), 3);
}

#[test]
fn sav_68_installed_changed_vanilla_reference_resets_and_round_trips() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping SAV-68 installed reference check");
        return;
    };
    let bytes = std::fs::read(data.join("Skyrim.esm")).unwrap();
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut world = World::new(order);
    let candidate = world
        .order()
        .winners_of(T(b"REFR"))
        .find_map(|(id, record)| {
            let base = record.find(T(b"NAME")).and_then(|bytes| {
                let raw = u32::from_le_bytes(bytes.try_into().ok()?);
                Some(GlobalFormId {
                    file: id.file,
                    object: raw,
                })
            })?;
            let base_record = world.order().winner(base)?;
            (base_record.tag == T(b"CONT") && base_record.find(T(b"CNTO")).is_some()).then_some(id)
        });
    let Some(reference) = candidate else {
        panic!("Skyrim.esm contains no placed container with base inventory");
    };
    let authored = world.reference(reference).unwrap().clone();
    let item = authored.inventory.iter().next().map(|(item, _)| *item);
    {
        let state = world.reference(reference).unwrap();
        state.position = [
            state.position[0] + 32.0,
            state.position[1] - 64.0,
            state.position[2] + 8.0,
        ];
        state.enabled = !state.enabled;
        state.opened = true;
        if let Some(item) = item {
            state.inventory.take(item, i32::MAX);
        }
    }
    assert!(world.reset_reference(reference, None));
    let reset = world.reference_ref(reference).unwrap();
    assert_eq!(reset.position, authored.position);
    assert_eq!(reset.rotation, authored.rotation);
    assert_eq!(reset.enabled, authored.enabled);
    assert_eq!(reset.inventory, authored.inventory);

    let saved = world.save_state();
    let mut restored_order = LoadOrder::new();
    restored_order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut restored = World::new(restored_order);
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.save_state(), saved);
    assert_eq!(restored.reference_ref(reference), Some(reset));
}
