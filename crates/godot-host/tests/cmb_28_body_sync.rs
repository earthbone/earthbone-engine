// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-28: the host keeps one server body per contact actor. `plan_body_sync`
//! is the pure decision the Godot adapter applies each tick, and
//! `weapon_segment` turns a posed weapon node into the swept segment.

use std::collections::HashMap;

use soe_esm::GlobalFormId;
use soe_game::combat::{Error, Hand};
use soe_game::world::ContactActor;
use soe_godot_host::combat::posed_weapon_bone;
use soe_godot_host::combat::{plan_body_sync, weapon_bones, weapon_segment, BodySyncPlan};
use soe_hkx::skeleton::{Bone, Skeleton};
use soe_hkx::QsTransform;

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
fn cmb_28_an_empty_host_creates_one_body_per_actor() {
    let wanted = [
        contact(0x900, 0x10, [0.0; 3], 1.0),
        contact(0x901, 0x10, [50.0, 0.0, 0.0], 1.2),
    ];
    let plan = plan_body_sync(&HashMap::new(), &wanted);
    assert_eq!(plan.create, wanted.to_vec());
    assert!(plan.moved.is_empty() && plan.removed.is_empty());
}

#[test]
fn cmb_28_a_cell_change_frees_the_left_cell_and_creates_the_new_one() {
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
fn cmb_28_a_scale_change_rebuilds_the_body_and_a_small_drift_does_not() {
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
fn cmb_28_a_duplicate_actor_is_planned_once() {
    let wanted = [
        contact(0x900, 0x10, [0.0; 3], 1.0),
        contact(0x900, 0x10, [9.0; 3], 1.0),
    ];
    let plan = plan_body_sync(&HashMap::new(), &wanted);
    assert_eq!(plan.create, vec![wanted[0]]);
}

#[test]
fn cmb_28_an_unchanged_world_plans_only_moves() {
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
fn cmb_28_the_segment_runs_reach_units_along_the_normalized_blade_axis() {
    let (origin, tip) = weapon_segment([10.0, 20.0, 30.0], [0.0, 0.0, -4.0], 150.0).unwrap();
    assert_eq!(origin, [10.0, 20.0, 30.0]);
    assert_eq!(tip, [10.0, 20.0, -120.0]);
    let (_, diagonal) = weapon_segment([0.0; 3], [3.0, 4.0, 0.0], 10.0).unwrap();
    assert!((diagonal[0] - 6.0).abs() < 1e-5 && (diagonal[1] - 8.0).abs() < 1e-5);
}

#[test]
fn cmb_28_a_degenerate_pose_or_reach_yields_no_segment() {
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
fn cmb_28_weapon_nodes_prefer_the_vanilla_attach_points_then_the_hand_bone() {
    assert_eq!(weapon_bones(Hand::Right)[0], "WEAPON");
    assert_eq!(weapon_bones(Hand::Left)[0], "SHIELD");
    assert_eq!(weapon_bones(Hand::Right).last(), Some(&"NPC R Hand [RHnd]"));
    assert_eq!(weapon_bones(Hand::Left).last(), Some(&"NPC L Hand [LHnd]"));
}

fn skeleton(names: &[&str]) -> Skeleton {
    Skeleton {
        name: "test".into(),
        parent_indices: vec![-1; names.len()],
        bones: names
            .iter()
            .map(|name| Bone {
                name: (*name).into(),
                lock_translation: false,
            })
            .collect(),
        reference_pose: vec![QsTransform::IDENTITY; names.len()],
        reference_floats: vec![],
        float_slots: vec![],
    }
}

#[test]
fn cmb_28_animated_weapon_pose_prefers_attach_node_and_maps_units() {
    let rig = skeleton(&["NPC R Hand [RHnd]", "WEAPON"]);
    let mut model = vec![QsTransform::IDENTITY; 2];
    model[0].translation = [900.0, 0.0, 0.0, 0.0];
    model[1].translation = [70.0, 140.0, 210.0, 0.0];
    let pose = posed_weapon_bone(&rig, &model, Hand::Right).unwrap();
    assert!((pose.origin[0] - 1.000_125).abs() < 1e-5);
    assert!((pose.origin[1] - 3.000_375).abs() < 1e-5);
    assert!((pose.origin[2] + 2.000_25).abs() < 1e-5);
}

#[test]
fn cmb_28_animated_weapon_pose_falls_back_to_hand_and_rejects_missing_pose() {
    let rig = skeleton(&["NPC L Hand [LHnd]"]);
    let model = [QsTransform::IDENTITY];
    assert_eq!(
        posed_weapon_bone(&rig, &model, Hand::Left).unwrap().origin,
        [0.0; 3]
    );
    assert!(matches!(
        posed_weapon_bone(&rig, &[], Hand::Left),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn cmb_28_bound_rig_placement_uses_world_rotation_position_and_scale() {
    let rotation =
        soe_godot_host::units::skyrim_euler_matrix([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
    let placed = soe_godot_host::units::placement_to_godot(rotation, [70.0, 140.0, 0.0], 1.5);
    assert!((placed.origin.x - 1.000_125).abs() < 1e-5);
    assert!((placed.origin.z + 2.000_25).abs() < 1e-5);
    assert!((-placed.basis[0][2] - 1.5).abs() < 1e-5);
}

#[test]
fn cmb_28_vanilla_one_hand_graph_emits_an_authored_hit_frame() {
    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let mut vfs = soe_bsa::Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&std::path::Path::new(&data_dir).join(archive))
            .unwrap();
    }
    let assets = soe_godot_host::actor::ActorAssets::load_with_behavior(
        &vfs,
        "meshes/actors/character/character assets/malebody_1.nif",
        soe_godot_host::actor::ONE_HAND_BEHAVIOR,
    )
    .unwrap();
    assert!(
        assets
            .graph
            .data
            .event_names
            .iter()
            .any(|name| name == "attackStart"),
        "the one-hand graph must declare the vanilla attack input"
    );
    let mut vm = soe_hkx::interp::Interpreter::new(
        &assets.graph,
        &assets.clips,
        assets.skeleton.reference_pose.clone(),
    );
    assert!(vm.variables.set_by_name("iRightHandType", 1.0));
    assert!(vm.send_event_named("attackStart"));
    let mut hit_frame = false;
    for _ in 0..180 {
        vm.update(1.0 / 60.0);
        hit_frame |= vm.take_events().iter().any(|event| {
            usize::try_from(event.id)
                .ok()
                .and_then(|id| assets.graph.data.event_names.get(id))
                .is_some_and(|name| name == "HitFrame")
        });
        if hit_frame {
            break;
        }
    }
    assert!(
        hit_frame,
        "the authored attack must reach a HitFrame marker"
    );
    assert!(
        vm.active_clips().iter().any(|(id, _)| assets
            .graph
            .node(*id)
            .name
            .contains("1HM_AttackRight")),
        "HitFrame must belong to the one-handed attack pose"
    );
}
