// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ai, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x14, 0x801] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("AI.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn coarse_process_clock_survives_world_save_and_load() {
    let mut world = world();
    world.player();
    world.actor(id(0x801)).unwrap();
    world.reference(id(0x14)).unwrap().position = [0.0; 3];
    world.reference(id(0x801)).unwrap().position = [20_000.0, 0.0, 0.0];
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    for _ in 0..30 {
        world.clock.real_seconds += 1.0 / 60.0;
        ai::tick(&mut world, 1.0 / 60.0, &mut alloc);
    }
    let before = world.save_state();
    assert!(world.actor_ref(id(0x801)).unwrap().process_clock.is_some());
    world.load_state(&before).unwrap();
    assert_eq!(world.save_state(), before);
    assert!(world.actor_ref(id(0x801)).unwrap().process_clock.is_some());
}

#[test]
fn combat_promotes_a_far_actor_to_high_process_cadence() {
    let mut world = world();
    world.player();
    world.actor(id(0x801)).unwrap();
    world.reference(id(0x14)).unwrap().position = [0.0; 3];
    world.reference(id(0x801)).unwrap().position = [20_000.0, 0.0, 0.0];
    world.actor(id(0x801)).unwrap().in_combat = true;
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    for _ in 0..4 {
        world.clock.real_seconds += 1.0 / 60.0;
        ai::tick(&mut world, 1.0 / 60.0, &mut alloc);
    }
    assert_eq!(world.ai.process_updates[0], 1);
    assert_eq!(world.ai.process_updates[3], 0);
}
