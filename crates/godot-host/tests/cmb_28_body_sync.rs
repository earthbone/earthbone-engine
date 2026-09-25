// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-28: the host keeps one server body per contact actor. `plan_body_sync`
//! is the pure decision the Godot adapter applies each tick, and
//! `weapon_segment` turns a posed weapon node into the swept segment.

use std::collections::HashMap;

use soe_esm::GlobalFormId;
use soe_game::combat::{Error, Hand};
use soe_game::world::ContactActor;
use soe_godot_host::combat::{plan_body_sync, weapon_bones, weapon_segment, BodySyncPlan};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn contact(object: u32, cell: u32, position: [f32; 3], scale: f32) -> ContactActor {
    ContactActor {
        actor: id(object),
        cell: id(cell),
        position,
        rotation: [0.0; 3],
        scale,
    }
}

#[test]
fn an_empty_host_creates_one_body_per_actor() {
    let wanted = [
        contact(0x900, 0x10, [0.0; 3], 1.0),
        contact(0x901, 0x10, [50.0, 0.0, 0.0], 1.2),
    ];
    let plan = plan_body_sync(&HashMap::new(), &wanted);
    assert_eq!(plan.create, wanted.to_vec());
    assert!(plan.moved.is_empty() && plan.removed.is_empty());
}

#[test]
fn a_cell_change_frees_the_left_cell_and_creates_the_new_one() {
    let current: HashMap<_, _> = [(id(0x14), 1.0), (id(0x900), 1.0)].into_iter().collect();
    let wanted = [
        contact(0x14, 0x11, [500.0, 10.0, 20.0], 1.0),
        contact(0x961, 0x11, [520.0, 0.0, 0.0], 1.0),
    ];
    let plan = plan_body_sync(&current, &wanted);
    assert_eq!(plan.moved, vec![wanted[0]], "the player's body only moves");
    assert_eq!(plan.create, vec![wanted[1]], "the far actor gets a body");
    assert_eq!(
        plan.removed,
        vec![id(0x900)],
        "the left actor's body is freed"
    );
}

#[test]
fn a_scale_change_rebuilds_the_body_and_a_small_drift_does_not() {
    let current: HashMap<_, _> = [(id(0x900), 1.0), (id(0x901), 1.0)].into_iter().collect();
    let wanted = [
        contact(0x900, 0x10, [0.0; 3], 1.5),
        contact(0x901, 0x10, [0.0; 3], 1.000_01),
    ];
    let plan = plan_body_sync(&current, &wanted);
    assert_eq!(plan.removed, vec![id(0x900)]);
    assert_eq!(plan.create, vec![wanted[0]]);
    assert_eq!(plan.moved, vec![wanted[1]]);
}

#[test]
fn a_duplicate_actor_is_planned_once() {
    let wanted = [
        contact(0x900, 0x10, [0.0; 3], 1.0),
        contact(0x900, 0x10, [9.0; 3], 1.0),
    ];
    let plan = plan_body_sync(&HashMap::new(), &wanted);
    assert_eq!(plan.create, vec![wanted[0]]);
}

#[test]
fn an_unchanged_world_plans_only_moves() {
    let current: HashMap<_, _> = [(id(0x900), 1.0)].into_iter().collect();
    let wanted = [contact(0x900, 0x10, [1.0; 3], 1.0)];
    let plan = plan_body_sync(&current, &wanted);
    assert_eq!(
        plan,
        BodySyncPlan {
            create: vec![],
            moved: wanted.to_vec(),
            removed: vec![],
        }
    );
}

#[test]
fn the_segment_runs_reach_units_along_the_normalized_blade_axis() {
    let (origin, tip) = weapon_segment([10.0, 20.0, 30.0], [0.0, 0.0, -4.0], 150.0).unwrap();
    assert_eq!(origin, [10.0, 20.0, 30.0]);
    assert_eq!(tip, [10.0, 20.0, -120.0]);
    let (_, diagonal) = weapon_segment([0.0; 3], [3.0, 4.0, 0.0], 10.0).unwrap();
    assert!((diagonal[0] - 6.0).abs() < 1e-5 && (diagonal[1] - 8.0).abs() < 1e-5);
}

#[test]
fn a_degenerate_pose_or_reach_yields_no_segment() {
    for (origin, axis, reach) in [
        ([0.0; 3], [0.0; 3], 100.0),
        ([f32::NAN, 0.0, 0.0], [0.0, 0.0, 1.0], 100.0),
        ([0.0; 3], [0.0, 0.0, f32::INFINITY], 100.0),
        ([0.0; 3], [0.0, 0.0, 1.0], 0.0),
        ([0.0; 3], [0.0, 0.0, 1.0], f32::NAN),
    ] {
        assert!(matches!(
            weapon_segment(origin, axis, reach),
            Err(Error::Number)
        ));
    }
}

#[test]
fn weapon_nodes_prefer_the_vanilla_attach_points_then_the_hand_bone() {
    assert_eq!(weapon_bones(Hand::Right)[0], "WEAPON");
    assert_eq!(weapon_bones(Hand::Left)[0], "SHIELD");
    assert_eq!(weapon_bones(Hand::Right).last(), Some(&"NPC R Hand [RHnd]"));
    assert_eq!(weapon_bones(Hand::Left).last(), Some(&"NPC L Hand [LHnd]"));
}
