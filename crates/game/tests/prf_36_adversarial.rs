// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial process cadence and waypoint worker checks for PRF-36.

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
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()),
        );
    }
    for cell in [0x900, 0x901] {
        plugin.push_top(Record::new(Tag(*b"CELL"), cell).sub(Tag(*b"DATA"), [1, 0]));
    }
    let mut order = LoadOrder::new();
    order.add("Prf36.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.player();
    world.actor(id(0x14)).unwrap();
    world.actor(id(0x801)).unwrap();
    world.reference(id(0x14)).unwrap().cell = Some(id(0x900));
    world.reference(id(0x801)).unwrap().cell = Some(id(0x901));
    world
}

#[test]
fn prf_36_same_interior_cells_use_their_shared_distance_band() {
    let mut world = world();
    world.reference(id(0x801)).unwrap().position = [100.0, 0.0, 0.0];
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    ai::tick(&mut world, 1.0 / 60.0, &mut alloc);
    assert_eq!(world.ai.process_counts, [2, 0, 0, 0]);
}

#[test]
fn prf_36_far_combat_actor_remains_high_process() {
    let mut world = world();
    world.reference(id(0x801)).unwrap().position = [50_000.0, 0.0, 0.0];
    world.actor(id(0x801)).unwrap().in_combat = true;
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    ai::tick(&mut world, 1.0 / 60.0, &mut alloc);
    assert_eq!(world.ai.process_counts, [2, 0, 0, 0]);
}

#[test]
fn prf_36_joined_waypoint_jobs_keep_motion_and_report_worker_execution() {
    let mut world = world();
    world.reference(id(0x14)).unwrap().cell = Some(id(0x900));
    world.reference(id(0x801)).unwrap().cell = Some(id(0x900));
    world.actor(id(0x801)).unwrap().ai = ai::AiState {
        procedure: ai::Procedure::Travel,
        speed: 100.0,
        destination: Some([100.0, 0.0, 0.0]),
        path: vec![[100.0, 0.0, 0.0]],
        ..ai::AiState::default()
    };
    world
        .ai
        .set_planning_workers(2.try_into().unwrap())
        .unwrap();
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    ai::tick(&mut world, 0.5, &mut alloc);
    assert!(world.reference_ref(id(0x801)).unwrap().position[0] > 0.0);
    assert_eq!(world.ai.movement_workers().jobs, 1);
    assert!(world
        .ai
        .movement_workers()
        .busy
        .iter()
        .any(|busy| !busy.is_zero()));
}
