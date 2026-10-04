// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionEvent, Motion};
use soe_game::world::ContactActor;
use soe_game::World;
use soe_godot_host::detection::{light_level, observations_in_range};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn actor(object: u32, cell: u32, position: [f32; 3]) -> ContactActor {
    ContactActor {
        actor: id(object),
        cell: id(cell),
        position,
        rotation: [0.0; 3],
        scale: 1.0,
    }
}

fn light_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 1));
    let mut light_data = vec![0_u8; 48];
    light_data[4..8].copy_from_slice(&100_u32.to_le_bytes());
    light_data[8..11].fill(255);
    plugin.push_top(Record::new(Tag(*b"LIGH"), 2).sub(b"DATA", light_data));
    let mut placement = vec![0_u8; 24];
    placement[..4].copy_from_slice(&10.0_f32.to_le_bytes());
    let mut reference = Record::new(Tag(*b"REFR"), 3)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"DATA", placement);
    reference.placement.cell = Some(1);
    plugin.push_top(reference);
    let mut order = LoadOrder::new();
    order.add("DetectionLight.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_light_sample_uses_current_enabled_and_moved_reference_state() {
    let mut world = light_world();
    let near = light_level(&world, id(1), [10.0, 0.0, 0.0]);
    assert!(near > 0.0);
    world.reference(id(3)).unwrap().enabled = false;
    assert_eq!(light_level(&world, id(1), [10.0, 0.0, 0.0]), 0.0);
    world.reference(id(3)).unwrap().enabled = true;
    world.reference(id(3)).unwrap().position = [1000.0, 0.0, 0.0];
    assert_eq!(light_level(&world, id(1), [10.0, 0.0, 0.0]), 0.0);
    assert!(light_level(&world, id(1), [1000.0, 0.0, 0.0]) > 0.0);
}

#[test]
fn prf_31_malformed_or_nonlight_placed_references_do_not_panic_or_add_light() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut ambient = vec![0_u8; 92];
    ambient[..3].fill(51);
    plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"XCLL", ambient));
    plugin.push_top(Record::new(Tag(*b"STAT"), 2));
    let mut nonlight = Record::new(Tag(*b"REFR"), 3).sub(b"NAME", 2_u32.to_le_bytes());
    nonlight.placement.cell = Some(1);
    plugin.push_top(nonlight);
    let mut malformed = Record::new(Tag(*b"REFR"), 4).sub(b"NAME", [1_u8, 2, 3]);
    malformed.placement.cell = Some(1);
    plugin.push_top(malformed);
    let mut order = LoadOrder::new();
    order.add("MalformedLights.esm", plugin).unwrap();
    let world = World::new(order);
    assert_eq!(light_level(&world, id(1), [0.0; 3]), 20.0);
}

#[test]
fn prf_31_spatial_pairs_include_exact_range_and_negative_bucket_boundary() {
    let actors = [
        actor(10, 1, [-100.0, -100.0, 0.0]),
        actor(11, 1, [0.0, -100.0, 0.0]),
        actor(12, 1, [0.01, -100.0, 0.0]),
        actor(13, 2, [-100.0, -100.0, 0.0]),
    ];
    let pairs = observations_in_range(
        &actors,
        &[],
        &BTreeMap::new(),
        100.0,
        |_, _| true,
        |_, _| 0.0,
    );
    assert!(pairs
        .iter()
        .any(|pair| pair.observer == id(10) && pair.target == id(11)));
    assert!(!pairs
        .iter()
        .any(|pair| pair.observer == id(10) && pair.target == id(12)));
    assert!(!pairs.iter().any(|pair| pair.observer == id(10) && pair.target == id(13)));
}

#[test]
fn prf_31_detection_event_is_sampled_at_its_location_and_owner_can_be_distant() {
    let actors = [actor(10, 1, [0.0; 3]), actor(11, 1, [1000.0, 0.0, 0.0])];
    let events = [
        DetectionEvent {
            owner: id(11),
            cell: id(1),
            position: [5.0, 0.0, 0.0],
            sound_level: 60.0,
        },
        DetectionEvent {
            owner: id(11),
            cell: id(1),
            position: [6.0, 0.0, 0.0],
            sound_level: 100.0,
        },
        DetectionEvent {
            owner: id(11),
            cell: id(1),
            position: [500.0, 0.0, 0.0],
            sound_level: 100.0,
        },
    ];
    let pairs = observations_in_range(
        &actors,
        &events,
        &BTreeMap::<GlobalFormId, Motion>::new(),
        100.0,
        |_, _| true,
        |_, _| panic!("sound only does not need a light sample"),
    );
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].target, id(11));
    assert_eq!(pairs[0].action_sound, 100.0);
    assert_eq!(pairs[0].action_sound_distance, Some(6.0));
    assert!(!pairs[0].line_of_sight);
}

#[test]
fn prf_31_sparse_actor_neighborhood_never_queries_distant_sight_or_light() {
    let actors = (10..2010)
        .map(|object| actor(object, 1, [object as f32 * 1000.0, 0.0, 0.0]))
        .collect::<Vec<_>>();
    let pairs = observations_in_range(
        &actors,
        &[],
        &BTreeMap::new(),
        100.0,
        |_, _| panic!("distant actors must not trigger sight rays"),
        |_, _| panic!("distant actors must not trigger light samples"),
    );
    assert!(pairs.is_empty());
}
