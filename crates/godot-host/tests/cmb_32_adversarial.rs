// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionEvent, Motion};
use soe_game::world::ContactActor;
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn light_world(reference_flags: u32, light_flags: u32) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", [0, 0, 0]));
    let mut data = Vec::new();
    data.extend_from_slice(&(-1_i32).to_le_bytes());
    data.extend_from_slice(&100_u32.to_le_bytes());
    data.extend_from_slice(&[255, 255, 255, 0]);
    data.extend_from_slice(&light_flags.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .with_flags(reference_flags)
        .sub(b"NAME", 2_u32.to_le_bytes());
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("Detection.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn cmb_32_initially_disabled_reference_does_not_emit_placed_light() {
    let world = light_world(flags::INITIALLY_DISABLED, 0);
    assert_eq!(
        soe_godot_host::detection::light_level(&world, id(1), [0.0; 3]),
        0.0
    );
}

#[test]
fn cmb_32_off_by_default_light_does_not_emit_until_enabled() {
    let world = light_world(0, 1 << 5);
    assert_eq!(
        soe_godot_host::detection::light_level(&world, id(1), [0.0; 3]),
        0.0
    );
}

#[test]
fn cmb_32_runtime_placed_light_changes_detection_light() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", [0, 0, 0]));
    let mut data = Vec::new();
    data.extend_from_slice(&(-1_i32).to_le_bytes());
    data.extend_from_slice(&100_u32.to_le_bytes());
    data.extend_from_slice(&[255, 255, 255, 0]);
    data.extend_from_slice(&0_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", data));
    plugin.push_top(Record::new(Tag(*b"STAT"), 3));
    let mut anchor = Record::new(Tag(*b"REFR"), 4)
        .sub(b"NAME", 3_u32.to_le_bytes())
        .sub(b"DATA", [0_u8; 24]);
    anchor.placement.cell = Some(1);
    plugin.push_top(anchor);
    let mut order = LoadOrder::new();
    order.add("DynamicLight.esm", plugin).unwrap();
    let mut world = World::new(order);
    let runtime_light = world.place_at(id(2), id(4), false);
    let placement = world.ref_placement(runtime_light).unwrap();
    let light = soe_godot_host::detection::light_level(&world, id(1), placement.position);
    assert!(
        light > 0.0,
        "an enabled runtime light raises local detection light"
    );
}

#[test]
fn cmb_32_event_at_emitter_position_keeps_owner_and_measures_distance() {
    let cell = id(1);
    let observer = ContactActor {
        actor: id(4),
        cell,
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: 1.0,
    };
    let owner = ContactActor {
        actor: id(5),
        position: [20.0, 0.0, 0.0],
        ..observer
    };
    let event = DetectionEvent {
        owner: owner.actor,
        cell,
        position: [3.0, 4.0, 0.0],
        sound_level: 125.0,
    };
    let values = soe_godot_host::detection::observations(
        &[observer, owner],
        &[event],
        &BTreeMap::from([(owner.actor, Motion::Running)]),
        |_, _| false,
        |_, _| 0.0,
    );
    let sampled = values
        .iter()
        .find(|value| value.observer == observer.actor && value.target == owner.actor)
        .unwrap();
    assert_eq!(sampled.action_sound, 125.0);
    assert_eq!(sampled.action_sound_distance, Some(5.0));
    assert_eq!(sampled.action_sound_line_of_sight, Some(false));
    assert_eq!(sampled.motion, Motion::Running);
}
