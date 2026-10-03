// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{EventState, FurnitureEventKind, HookKind, UnsupportedHook};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::ActorHandle;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fresh() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x802));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), 0x803)
            .sub(Tag(*b"MNAM"), 1_u32.to_le_bytes())
            .sub(
                Tag(*b"FNPR"),
                [1_u16.to_le_bytes().as_slice(), &[0, 0]].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x902).sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn shm_21_event_state_reads_live_inventory_actor_deletion_and_furniture() {
    let game = fresh();
    let mut world = game.world_mut();
    let actor = id(0x900);
    let container = id(0x901);
    let item = id(0x802);
    let furniture_id = id(0x902);
    world
        .reference(container)
        .unwrap()
        .inventory
        .add(item, 4)
        .unwrap();
    world.reference(furniture_id).unwrap();
    let actor_state = world.actor(actor).unwrap();
    actor_state.unconscious = true;
    assert!(world.kill_actor(actor, None));
    assert_eq!(world.reserve_furniture_marker(furniture_id, actor), Some(0));
    world.reference(container).unwrap().deleted = true;

    let actor_handle = ActorHandle(world.runtime_id(actor));
    assert_eq!(
        world.inventory_count(
            FormId(world.runtime_id(container)),
            FormId(world.runtime_id(item))
        ),
        Some(4)
    );
    assert_eq!(world.is_dead(actor_handle), Some(true));
    assert_eq!(world.is_in_bleedout(actor_handle), Some(false));
    let alive_actor = id(0x900);
    world.actor(alive_actor).unwrap().dead = false;
    assert_eq!(world.is_in_bleedout(actor_handle), Some(true));
    assert_eq!(
        world.is_form_deleted(FormId(world.runtime_id(container))),
        Some(true)
    );
    let furniture = world
        .furniture_event(actor_handle)
        .expect("the current actor seat is exposed as furniture occupancy");
    assert_eq!(furniture.kind, FurnitureEventKind::Enter);
    assert_eq!(furniture.target_furniture.0, world.runtime_id(id(0x902)));

    assert_eq!(world.inventory_count(FormId(u32::MAX), FormId(1)), None);
    assert_eq!(world.is_dead(ActorHandle(u32::MAX)), None);
    assert_eq!(world.is_in_bleedout(ActorHandle(u32::MAX)), None);
    assert_eq!(world.is_form_deleted(FormId(u32::MAX)), None);
    assert_eq!(world.furniture_event(ActorHandle(u32::MAX)), None);
}

#[test]
fn shm_21_unsupported_hook_is_reported_by_the_game_adapter() {
    let game = fresh();
    let error: UnsupportedHook = game
        .world()
        .reject_commonlib_hook("TESFurnitureEvent vtable")
        .unwrap_err();
    assert_eq!(error.name, "TESFurnitureEvent vtable");
    assert_eq!(error.reason, error.name);
    assert_eq!(
        soe_commonlib_shim::events::request_hook(HookKind::Vtable),
        Err(soe_commonlib_shim::events::HookError::Unsupported(
            HookKind::Vtable
        ))
    );
}
