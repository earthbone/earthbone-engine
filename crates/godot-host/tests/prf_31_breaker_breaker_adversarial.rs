// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionEvent, Motion};
use soe_game::world::ContactActor;
use soe_game::World;
use soe_godot_host::detection::DetectionSpaces;

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

fn exterior_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"WRLD"), 100));
    plugin.push_top(Record::new(Tag(*b"WRLD"), 101));
    for (cell, world, x) in [(1, 100, 0_i32), (2, 100, 1), (3, 101, 0)] {
        let mut grid = x.to_le_bytes().to_vec();
        grid.extend_from_slice(&0_i32.to_le_bytes());
        let mut record = Record::new(Tag(*b"CELL"), cell).sub(b"XCLC", grid);
        record.placement.world = Some(world);
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("DetectionSpaces.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prf_31_sound_from_sibling_exterior_cell_uses_source_and_owner_separately() {
    let world = exterior_world();
    let actors = vec![
        actor(10, 1, [0.0; 3]),
        actor(11, 2, [20_000.0, 0.0, 0.0]),
    ];
    let events = [DetectionEvent {
        owner: id(11),
        cell: id(2),
        position: [10.0, 0.0, 0.0],
        sound_level: 125.0,
    }];
    let spaces = DetectionSpaces::new(&world, &actors, &events, 100.0);
    let candidates = spaces.candidate_actors(actors, &events);
    assert_eq!(candidates.len(), 2);
    let mut sight_calls = 0;
    let observations = spaces.observations(
        &candidates,
        &events,
        &BTreeMap::<GlobalFormId, Motion>::new(),
        |_, source| {
            sight_calls += 1;
            assert_eq!(source.actor, id(11));
            assert_eq!(source.position, [10.0, 0.0, 0.0]);
            false
        },
        |_, _| panic!("a sound without a nearby actor does not sample light"),
    );
    let heard = observations
        .iter()
        .find(|observation| observation.observer == id(10) && observation.target == id(11))
        .unwrap();
    assert_eq!(heard.action_sound, 125.0);
    assert_eq!(heard.action_sound_distance, Some(10.0));
    assert_eq!(heard.action_sound_line_of_sight, Some(false));
    assert_eq!(sight_calls, 1);
}

#[test]
fn prf_31_equal_coordinates_in_different_worldspaces_do_not_share_sounds() {
    let world = exterior_world();
    let actors = vec![actor(10, 1, [0.0; 3]), actor(11, 3, [20_000.0, 0.0, 0.0])];
    let events = [DetectionEvent {
        owner: id(11),
        cell: id(3),
        position: [1.0, 0.0, 0.0],
        sound_level: 100.0,
    }];
    let spaces = DetectionSpaces::new(&world, &actors, &events, 100.0);
    let candidates = spaces.candidate_actors(actors, &events);
    let observations = spaces.observations(
        &candidates,
        &events,
        &BTreeMap::<GlobalFormId, Motion>::new(),
        |_, _| panic!("different worldspaces do not form sight pairs"),
        |_, _| panic!("different worldspaces do not sample light"),
    );
    assert!(!observations
        .iter()
        .any(|observation| observation.observer == id(10) && observation.target == id(11)));
}
