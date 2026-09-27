// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use soe_esm::GlobalFormId;
use soe_game::combat::Hand;
use soe_game::world::ContactActor;
use soe_godot_host::combat::{plan_body_sync, posed_weapon_bone};
use soe_hkx::skeleton::{Bone, Skeleton};
use soe_hkx::QsTransform;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn skeleton(names: &[&str]) -> Skeleton {
    Skeleton {
        name: "adversarial".into(),
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
fn cmb_28_rotated_attachment_beats_hand_and_preserves_blade_axis() {
    let rig = skeleton(&["NPC R Hand [RHnd]", "WEAPON"]);
    let mut pose = vec![QsTransform::IDENTITY; 2];
    pose[0].translation = [700.0, 0.0, 0.0, 0.0];
    let half = std::f32::consts::FRAC_PI_4;
    pose[1].translation = [70.0, 140.0, 210.0, 0.0];
    pose[1].rotation = [0.0, 0.0, half.sin(), half.cos()];

    let weapon = posed_weapon_bone(&rig, &pose, Hand::Right).unwrap();
    assert!((weapon.origin[0] - 1.000125).abs() < 1e-5);
    assert!((weapon.origin[1] - 3.000375).abs() < 1e-5);
    assert!((weapon.origin[2] + 2.00025).abs() < 1e-5);
    let blade_axis = [-weapon.basis[0][2], -weapon.basis[1][2], -weapon.basis[2][2]];
    assert!((blade_axis[0] + 1.0).abs() < 1e-5);
    assert!(blade_axis[1].abs() < 1e-5);
    assert!(blade_axis[2].abs() < 1e-5);
}

#[test]
fn cmb_28_cell_transition_moves_the_same_actor_body_and_removes_departures() {
    let actor = |cell, position| ContactActor {
        actor: id(0x900),
        cell: id(cell),
        position,
        rotation: [0.0; 3],
        scale: 1.0,
    };
    let current: HashMap<_, _> = [(id(0x900), 1.0), (id(0x901), 1.0)].into_iter().collect();
    let plan = plan_body_sync(&current, &[actor(0x20, [1.0, 2.0, 3.0])]);
    assert!(plan.create.is_empty());
    assert_eq!(plan.moved, vec![actor(0x20, [1.0, 2.0, 3.0])]);
    assert_eq!(plan.removed, vec![id(0x901)]);
}
