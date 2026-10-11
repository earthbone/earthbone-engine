// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world_with_scenery(count: u32) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"DATA", [1, 0]));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 2));
    let mut actor = Record::new(Tag(*b"ACHR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0; 24]);
    actor.placement.cell = Some(1);
    plugin.push_top(actor);
    plugin.push_top(Record::new(Tag(*b"STAT"), 4));
    for object in 10..10 + count {
        let mut reference = Record::new(Tag(*b"REFR"), object)
            .sub(b"NAME", 4_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]);
        reference.placement.cell = Some(1);
        plugin.push_top(reference);
    }
    let mut order = LoadOrder::new();
    order.add("DetectionIndex.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(1));
    world
}

#[test]
fn prf_31_actor_selection_survives_live_eligibility_changes_with_scenery() {
    let mut world = world_with_scenery(5_000);
    let placements = world.contact_actor_placements();
    assert_eq!(
        placements
            .iter()
            .map(|actor| actor.actor)
            .collect::<Vec<_>>(),
        [id(3)]
    );

    world.reference(id(3)).unwrap().is_actor = false;
    assert!(world.contact_actor_placements().is_empty());

    world.reference(id(3)).unwrap().is_actor = true;
    assert_eq!(world.contact_actor_placements()[0].actor, id(3));
}

#[test]
fn prf_31_actor_selection_rechecks_base_after_index_build() {
    let mut world = world_with_scenery(0);
    assert_eq!(world.contact_actor_placements().len(), 1);

    world.reference(id(3)).unwrap().base = None;
    assert!(world.contact_actor_placements().is_empty());

    world.reference(id(3)).unwrap().base = Some(id(2));
    assert_eq!(world.contact_actor_placements()[0].actor, id(3));
}
