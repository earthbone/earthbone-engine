// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::objective_map;

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn quest() -> Record {
    let mut record = Record::new(tag(*b"QUST"), 0x100).sub(tag(*b"DNAM"), [0; 12]);
    for (objective, alias, ignores_locks) in [(10_u16, 1_i32, 1_u8), (20, 2, 0)] {
        let mut target = alias.to_le_bytes().to_vec();
        target.extend_from_slice(&[ignores_locks, 0, 0, 0]);
        record = record
            .sub(tag(*b"QOBJ"), objective.to_le_bytes())
            .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
            .zsub(tag(*b"NNAM"), "Find the objective")
            .sub(tag(*b"QSTA"), target);
        if objective == 10 {
            let mut condition = vec![0; 32];
            condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
            condition[8..10].copy_from_slice(&58_u16.to_le_bytes());
            condition[12..16].copy_from_slice(&0x100_u32.to_le_bytes());
            record = record.sub(tag(*b"CTDA"), condition);
        }
    }
    record
        .sub(tag(*b"ALST"), 1_u32.to_le_bytes())
        .zsub(tag(*b"ALID"), "ReferenceTarget")
        .sub(tag(*b"ALFR"), 0x300_u32.to_le_bytes())
        .sub(tag(*b"ALED"), [])
        .sub(tag(*b"ALLS"), 2_u32.to_le_bytes())
        .zsub(tag(*b"ALID"), "LocationTarget")
        .sub(tag(*b"ALFL"), 0x200_u32.to_le_bytes())
        .sub(tag(*b"ALED"), [])
}

fn reference(form: u32, x: f32, y: f32, z: f32) -> Record {
    let mut transform = Vec::new();
    for value in [x, y, z, 0.0, 0.0, 0.0] {
        transform.extend_from_slice(&value.to_le_bytes());
    }
    Record::new(tag(*b"REFR"), form)
        .sub(tag(*b"NAME"), 0x400_u32.to_le_bytes())
        .sub(tag(*b"DATA"), transform)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest());
    plugin.push_top(Record::new(tag(*b"STAT"), 0x400));
    plugin.push_top(reference(0x300, 11.0, 22.0, 33.0));
    plugin.push_top(reference(0x301, 44.0, 55.0, 66.0));
    plugin.push_top(Record::new(tag(*b"LCTN"), 0x200).sub(tag(*b"MNAM"), 0x301_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("UiObjectiveMap.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.start_quest(id(0x100)));
    {
        let mut world = game.world_mut();
        let state = world.quest_state(id(0x100)).unwrap();
        state.highest = 1;
        world.objective(id(0x100), 10).unwrap().displayed = true;
        world.objective(id(0x100), 20).unwrap().displayed = true;
    }
    game
}

#[test]
fn ui_10_resolves_reference_and_location_alias_targets() {
    let mut game = game();

    let markers = objective_map::markers(&mut game);

    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].reference, id(0x300));
    assert_eq!(markers[0].position, [11.0, 22.0, 33.0]);
    assert!(markers[0].compass_marker_ignores_locks);
    assert_eq!(markers[1].reference, id(0x301));
    assert_eq!(markers[1].position, [44.0, 55.0, 66.0]);
    assert!(!markers[1].compass_marker_ignores_locks);
}

#[test]
fn ui_10_marker_visibility_tracks_objective_and_reference_world_state() {
    let mut game = game();
    assert!(objective_map::markers(&mut game)
        .iter()
        .all(|marker| marker.visible));

    game.world_mut().objective(id(0x100), 10).unwrap().displayed = false;
    game.world_mut().reference(id(0x301)).unwrap().enabled = false;

    let markers = objective_map::markers(&mut game);
    assert!(!markers[0].visible);
    assert!(!markers[1].visible);
}

#[test]
fn ui_10_target_conditions_control_objective_map_targets() {
    let mut game = game();
    assert_eq!(objective_map::markers(&mut game).len(), 2);

    game.world_mut().quest_state(id(0x100)).unwrap().highest = 0;

    let markers = objective_map::markers(&mut game);
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].objective, 20);
}

#[test]
fn ui_10_map_control_projects_world_xy_north_up_and_keeps_hidden_bounds() {
    let mut game = game();
    let markers = objective_map::markers(&mut game);
    let points = objective_map::project(&markers, 640.0, 360.0);

    assert_eq!(points.len(), 2);
    assert_eq!((points[0].x, points[0].y), (24.0, 336.0));
    assert_eq!((points[1].x, points[1].y), (616.0, 24.0));
    assert!(points.iter().all(|point| point.visible));

    game.world_mut().objective(id(0x100), 10).unwrap().displayed = false;
    let updated = objective_map::project(&objective_map::markers(&mut game), 640.0, 360.0);
    assert_eq!((updated[1].x, updated[1].y), (616.0, 24.0));
    assert!(!updated[0].visible);
    assert!(updated[1].visible);
}

#[test]
fn ui_10_map_control_handles_empty_and_degenerate_marker_sets() {
    let mut game = game();
    let marker = objective_map::markers(&mut game).remove(0);

    assert_eq!(objective_map::project(&[], 640.0, 360.0).len(), 0);
    let points = objective_map::project(std::slice::from_ref(&marker), 640.0, 360.0);
    assert_eq!((points[0].x, points[0].y), (320.0, 180.0));
    assert_eq!(
        objective_map::project(std::slice::from_ref(&marker), f32::NAN, 360.0).len(),
        0
    );
}
