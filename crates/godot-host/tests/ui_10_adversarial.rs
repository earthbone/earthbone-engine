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
    for objective in [10_u16, 20] {
        let alias = if objective == 10 { 1_i32 } else { 2 };
        let mut target = alias.to_le_bytes().to_vec();
        target.extend_from_slice(&[0, 0, 0, 0]);
        record = record
            .sub(tag(*b"QOBJ"), objective.to_le_bytes())
            .sub(tag(*b"FNAM"), 0_u32.to_le_bytes())
            .zsub(tag(*b"NNAM"), "Objective")
            .sub(tag(*b"QSTA"), target);
    }
    record
        .sub(tag(*b"ALST"), 1_u32.to_le_bytes())
        .zsub(tag(*b"ALID"), "ReferenceTarget")
        .sub(tag(*b"ALFR"), 0x300_u32.to_le_bytes())
        .sub(tag(*b"ALED"), [])
        .sub(tag(*b"ALLS"), 2_u32.to_le_bytes())
        .zsub(tag(*b"ALID"), "LocationTarget")
        .sub(tag(*b"ALFR"), 0x200_u32.to_le_bytes())
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

fn game_with_location_marker(mnam: Option<u32>, include_marker: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest());
    plugin.push_top(Record::new(tag(*b"STAT"), 0x400));
    plugin.push_top(reference(0x300, 11.0, 22.0, 33.0));
    if include_marker {
        plugin.push_top(reference(0x301, 44.0, 55.0, 66.0));
    }
    let mut location = Record::new(tag(*b"LCTN"), 0x200);
    if let Some(marker) = mnam {
        location = location.sub(tag(*b"MNAM"), marker.to_le_bytes());
    }
    plugin.push_top(location);
    let mut order = LoadOrder::new();
    order.add("UiObjectiveMapAdversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        let state = world.quest_state(id(0x100)).unwrap();
        state.active = true;
        state.highest = 1;
        world.objective(id(0x100), 10).unwrap().displayed = true;
        world.objective(id(0x100), 20).unwrap().displayed = true;
    }
    game
}

#[test]
fn ui_10_missing_location_mnam_omits_location_target_without_fallback() {
    let mut game = game_with_location_marker(None, true);

    let markers = objective_map::markers(&mut game);

    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].objective, 10);
    assert_eq!(markers[0].reference, id(0x300));
}

#[test]
fn ui_10_missing_mnam_reference_omits_location_target_without_fallback() {
    let mut game = game_with_location_marker(Some(0x301), false);

    let markers = objective_map::markers(&mut game);

    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].reference, id(0x300));
}

#[test]
fn ui_10_marker_reads_current_position_and_enabled_deleted_state_on_each_call() {
    let mut game = game_with_location_marker(Some(0x301), true);
    let first = objective_map::markers(&mut game);
    assert_eq!(first[1].position, [44.0, 55.0, 66.0]);
    assert!(first[1].visible);

    {
        let mut world = game.world_mut();
        let marker = world.reference(id(0x301)).unwrap();
        marker.position = [77.0, -88.0, 99.0];
        marker.enabled = false;
    }
    let second = objective_map::markers(&mut game);
    assert_eq!(second[1].position, [77.0, -88.0, 99.0]);
    assert!(!second[1].visible);

    {
        let mut world = game.world_mut();
        let marker = world.reference(id(0x301)).unwrap();
        marker.enabled = true;
        marker.deleted = true;
    }
    assert!(!objective_map::markers(&mut game)[1].visible);
}

#[test]
fn ui_10_inactive_quest_has_no_objective_map_markers() {
    let mut game = game_with_location_marker(Some(0x301), true);
    game.world_mut().quest_state(id(0x100)).unwrap().active = false;

    assert!(objective_map::markers(&mut game).is_empty());
}
