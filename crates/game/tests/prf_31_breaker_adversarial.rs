// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"DATA", [1, 0]));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 2));
    for object in [3_u32, 4] {
        let mut actor = Record::new(Tag(*b"ACHR"), object)
            .sub(b"NAME", 2_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]);
        actor.placement.cell = Some(1);
        plugin.push_top(actor);
    }
    let mut order = LoadOrder::new();
    order.add("ContactIndex.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(1));
    world
}

#[test]
fn prf_31_breaker_contact_index_coalesces_repeated_actor_changes() {
    let mut world = world();
    assert_eq!(
        world
            .contact_actor_placements()
            .iter()
            .map(|actor| actor.actor)
            .collect::<Vec<_>>(),
        [id(3), id(4)]
    );

    world.reference(id(3)).unwrap().is_actor = false;
    world.reference(id(3)).unwrap().is_actor = true;
    world.reference(id(3)).unwrap().is_actor = false;
    assert_eq!(
        world
            .contact_actor_placements()
            .iter()
            .map(|actor| actor.actor)
            .collect::<Vec<_>>(),
        [id(4)]
    );

    world.reference(id(3)).unwrap().is_actor = true;
    assert_eq!(
        world
            .contact_actor_placements()
            .iter()
            .map(|actor| actor.actor)
            .collect::<Vec<_>>(),
        [id(3), id(4)]
    );
}
