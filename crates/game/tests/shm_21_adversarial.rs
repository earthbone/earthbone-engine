// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{EventState, HookKind};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::ActorHandle;
use soe_esm::plugin::flags;
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
            .sub(Tag(*b"FNPR"), [1_u16.to_le_bytes().as_slice(), &[0, 0]].concat()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x902).sub(Tag(*b"NAME"), 0x803_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn inventory_count_reads_each_current_value() {
    let game = fresh();
    let mut world = game.world_mut();
    let container = id(0x901);
    let item = id(0x802);
    world
        .reference(container)
        .unwrap()
        .inventory
        .add(item, 3)
        .unwrap();
    let container_handle = FormId(world.runtime_id(container));
    let item_handle = FormId(world.runtime_id(item));
    assert_eq!(world.inventory_count(container_handle, item_handle), Some(3));
    world
        .reference(container)
        .unwrap()
        .inventory
        .add(item, 2)
        .unwrap();
    assert_eq!(world.inventory_count(container_handle, item_handle), Some(5));
}

#[test]
fn unconscious_actor_above_bleedout_threshold_is_not_in_bleedout() {
    let game = fresh();
    let mut world = game.world_mut();
    let actor = id(0x900);
    world.set_actor_value(actor, "health", 100.0).unwrap();
    world.actor(actor).unwrap().unconscious = true;
    assert!(!world.actor_is_bleeding_out(actor));
    assert_eq!(
        world.is_in_bleedout(ActorHandle(world.runtime_id(actor))),
        Some(false)
    );
}

#[test]
fn furniture_query_tracks_seat_release() {
    let game = fresh();
    let mut world = game.world_mut();
    let actor = id(0x900);
    let furniture = id(0x902);
    world.reference(furniture).unwrap();
    assert_eq!(world.reserve_furniture_marker(furniture, actor), Some(0));
    let actor_handle = ActorHandle(world.runtime_id(actor));
    let event = world.furniture_event(actor_handle).unwrap();
    assert_eq!(event.target_furniture.0, world.runtime_id(furniture));
    assert!(world.stand_from_furniture(actor));
    assert_eq!(world.furniture_event(actor_handle), None);
}

#[test]
fn deletion_query_reads_winning_record_deleted_flag() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut deleted = Record::new(Tag(*b"MISC"), 0x801);
    deleted.flags = flags::DELETED;
    plugin.push_top(deleted);
    let mut order = LoadOrder::new();
    order.add("Deleted.esm", plugin).unwrap();
    let game = Game::new(order);
    let world = game.world();
    assert_eq!(
        world.is_form_deleted(FormId(world.runtime_id(id(0x801)))),
        Some(true)
    );
}

#[test]
fn each_binary_hook_kind_is_explicitly_unsupported() {
    for kind in [HookKind::Vtable, HookKind::CallSite, HookKind::Branch] {
        assert_eq!(
            soe_commonlib_shim::events::request_hook(kind),
            Err(soe_commonlib_shim::events::HookError::Unsupported(kind))
        );
    }
    let game = fresh();
    let error = game.world().reject_commonlib_hook("test hook").unwrap_err();
    assert_eq!(error.name, "test hook");
    assert!(!error.reason.is_empty());
}
