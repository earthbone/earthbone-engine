// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::Motion;
use soe_game::world::ContactActor;
use soe_game::World;
use soe_godot_host::detection::{light_level, observations};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn actor(form: GlobalFormId, cell: GlobalFormId, position: [f32; 3]) -> ContactActor {
    ContactActor {
        actor: form,
        cell,
        position,
        rotation: [0.0; 3],
        scale: 1.0,
    }
}

fn world_with_cell_light(xcll: Vec<u8>, ltmp: Option<u32>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cell = Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", xcll);
    if let Some(template) = ltmp {
        cell = cell.sub(b"LTMP", template.to_le_bytes());
    }
    plugin.push_top(cell);
    let mut order = LoadOrder::new();
    order.add("Detection.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn cmb_32_truncated_cell_lighting_is_not_used_as_ambient() {
    let world = world_with_cell_light(vec![255, 255, 255], None);
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 0.0);
}

#[test]
fn cmb_32_truncated_light_data_does_not_emit_a_placed_light() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", [60, 60, 60]));
    let mut data = Vec::new();
    data.extend_from_slice(&(-1_i32).to_le_bytes());
    data.extend_from_slice(&100_u32.to_le_bytes());
    data.extend_from_slice(&[255, 255, 255, 0]);
    data.extend_from_slice(&0_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("Detection.esm", plugin).unwrap();
    let world = World::new(order);

    assert_eq!(light_level(&world, id(1), [0.0; 3]), 60.0 * (100.0 / 255.0));
}

#[test]
fn cmb_32_actor_identity_includes_plugin_file_for_same_object_number() {
    let cell = id(1);
    let first = actor(
        GlobalFormId { file: 1, object: 9 },
        cell,
        [0.0, 0.0, 0.0],
    );
    let second = actor(
        GlobalFormId { file: 2, object: 9 },
        cell,
        [10.0, 0.0, 0.0],
    );
    let sampled = observations(
        &[first, second],
        &[],
        &BTreeMap::from([(second.actor, Motion::Running)]),
        |_, _| true,
        |_, _| 0.0,
    );

    assert_eq!(sampled.len(), 2);
    assert_eq!((sampled[0].observer, sampled[0].target), (first.actor, second.actor));
    assert_eq!(sampled[0].motion, Motion::Running);
}

#[test]
fn cmb_32_repeated_samples_read_current_occlusion_and_light() {
    let cell = id(1);
    let observer = actor(id(4), cell, [0.0, 0.0, 0.0]);
    let target = actor(id(5), cell, [10.0, 0.0, 0.0]);
    let first = observations(
        &[observer, target],
        &[],
        &BTreeMap::new(),
        |_, _| false,
        |_, _| 10.0,
    );
    let second = observations(
        &[observer, target],
        &[],
        &BTreeMap::new(),
        |_, _| true,
        |_, _| 90.0,
    );

    assert!(!first[0].line_of_sight);
    assert_eq!(first[0].light_level, 10.0);
    assert!(second[0].line_of_sight);
    assert_eq!(second[0].light_level, 90.0);
}
