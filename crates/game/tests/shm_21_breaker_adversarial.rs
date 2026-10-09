// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{EventState, FurnitureEventKind};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::ActorHandle;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), 0x802)
            .sub(Tag(*b"MNAM"), 1_u32.to_le_bytes())
            .sub(
                Tag(*b"FNPR"),
                [1_u16.to_le_bytes().as_slice(), &[0, 0]].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x920).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm21Breaker.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn shm_21_event_queries_distinguish_zero_from_missing_inventory() {
    let game = fixture();
    let mut world = game.world_mut();
    world.reference(id(0x910)).unwrap();
    let container = FormId(world.runtime_id(id(0x910)));
    let item = FormId(world.runtime_id(id(0x801)));
    assert_eq!(world.inventory_count(container, item), Some(0));
    world.reference(id(0x910)).unwrap().inventory.add(id(0x801), 2).unwrap();
    assert_eq!(world.inventory_count(container, item), Some(2));
    assert_eq!(world.inventory_count(FormId(u32::MAX), item), None);
}

#[test]
fn shm_21_event_queries_reject_non_actor_handles_and_accept_false_state() {
    let game = fixture();
    let mut world = game.world_mut();
    world.actor(id(0x900)).unwrap();
    world.reference(id(0x910)).unwrap();
    let actor = ActorHandle(world.runtime_id(id(0x900)));
    let reference = ActorHandle(world.runtime_id(id(0x910)));
    assert_eq!(world.is_dead(actor), Some(false));
    assert_eq!(world.is_in_bleedout(actor), Some(false));
    assert_eq!(world.is_dead(reference), None);
    assert_eq!(world.is_in_bleedout(reference), None);
    assert_eq!(world.furniture_event(reference), None);
}

#[test]
fn shm_21_deleted_query_tracks_reference_mutation_and_still_resolves_it() {
    let game = fixture();
    let mut world = game.world_mut();
    let reference = FormId(world.runtime_id(id(0x910)));
    assert_eq!(world.is_form_deleted(reference), Some(false));
    world.reference(id(0x910)).unwrap().deleted = true;
    assert_eq!(world.is_form_deleted(reference), Some(true));
    assert_eq!(world.is_form_deleted(FormId(u32::MAX)), None);
}

#[test]
fn shm_21_furniture_query_tracks_seat_release_and_disabled_target() {
    let game = fixture();
    let mut world = game.world_mut();
    let actor = id(0x900);
    let handle = ActorHandle(world.runtime_id(actor));
    let furniture = id(0x920);
    assert_eq!(world.furniture_event(handle), None);
    assert_eq!(world.reserve_furniture_marker(furniture, actor), Some(0));
    for _ in 0..3 {
        let event = world.furniture_event(handle).unwrap();
        assert_eq!(event.kind, FurnitureEventKind::Enter);
        assert_eq!(event.actor.0, world.runtime_id(actor));
        assert_eq!(event.target_furniture.0, world.runtime_id(furniture));
    }
    assert!(world.stand_from_furniture(actor));
    assert_eq!(world.furniture_event(handle), None);
    assert_eq!(world.reserve_furniture_marker(furniture, actor), Some(0));
    world.reference(furniture).unwrap().enabled = false;
    assert_eq!(world.furniture_event(handle), None);
}

#[test]
fn shm_21_bleedout_queries_do_not_turn_dead_actors_into_unconscious_actors() {
    let game = fixture();
    let mut world = game.world_mut();
    world
        .settings
        .insert("fbleedoutdefault".into(), soe_papyrus::Value::Float(0.2));
    let actor = id(0x900);
    let handle = ActorHandle(world.runtime_id(actor));
    world.actor(actor).unwrap().unconscious = true;
    assert_eq!(world.is_in_bleedout(handle), Some(true));
    assert_eq!(world.is_in_bleedout(handle), Some(true));
    assert!(world.kill_actor(actor, None));
    assert_eq!(world.is_dead(handle), Some(true));
    assert_eq!(world.is_in_bleedout(handle), Some(false));
}
