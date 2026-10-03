// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::EventState;
use soe_commonlib_shim::{form::FormId, refr::ActorHandle};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn event_state_handles_follow_current_container_and_reject_non_actor_dead_queries() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm21State.esp", plugin).unwrap();
    let game = Game::new(order);
    let mut world = game.world_mut();
    world
        .reference(id(0x901))
        .unwrap()
        .inventory
        .add(id(0x801), 2)
        .unwrap();
    let container = FormId(world.runtime_id(id(0x901)));
    let item = FormId(world.runtime_id(id(0x801)));
    assert_eq!(world.inventory_count(container, item), Some(2));
    world
        .reference(id(0x901))
        .unwrap()
        .inventory
        .add(id(0x801), 3)
        .unwrap();
    assert_eq!(world.inventory_count(container, item), Some(5));
    assert_eq!(
        world.is_dead(ActorHandle(world.runtime_id(id(0x901)))),
        None
    );
}
