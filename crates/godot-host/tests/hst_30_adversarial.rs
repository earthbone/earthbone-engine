// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for NPC collision-grounded character bodies.

use std::collections::BTreeSet;

use soe_esm::GlobalFormId;
use soe_godot_host::npc_grounding::NpcGrounding;
use soe_nif::types::BlockRef;
use soe_physics::{Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3};

const CELL_A: GlobalFormId = GlobalFormId { file: 0, object: 2 };
const CELL_B: GlobalFormId = GlobalFormId { file: 0, object: 3 };
const ACTOR_A: GlobalFormId = GlobalFormId {
    file: 0,
    object: 10,
};
const ACTOR_B: GlobalFormId = GlobalFormId {
    file: 0,
    object: 11,
};

fn floor(x: f32, z: f32, half_width: f32) -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: "floor".into(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: Iso::placement([x, 0.0, z - 1.0], [0.0; 3], 1.0),
        shape: Shape::Box {
            half_extents: Vec3::new(half_width, 256.0, 1.0),
        },
    }
}

#[test]
fn scaled_npc_capsule_keeps_all_dimensions_and_recreates_on_scale_change() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0, 0.0, 512.0));
    let mut grounding = NpcGrounding::default();

    assert_eq!(
        grounding.update(
            &mut physics,
            ACTOR_A,
            Some(CELL_A),
            [0.0, 0.0, 80.0],
            0.5,
            0.0
        ),
        Some([0.0, 0.0, 0.0])
    );
    let small = grounding.character(&physics, ACTOR_A).unwrap();
    assert_eq!(small.settings.radius, 16.0);
    assert_eq!(small.settings.height, 64.0);
    assert_eq!(small.settings.crouch_height, 36.0);
    assert_eq!(small.settings.step_height, 18.0);

    grounding
        .update(
            &mut physics,
            ACTOR_A,
            Some(CELL_A),
            [0.0, 0.0, 80.0],
            1.0,
            0.0,
        )
        .unwrap();
    let full = grounding.character(&physics, ACTOR_A).unwrap();
    assert_eq!(full.settings.radius, 32.0);
    assert_eq!(full.settings.height, 128.0);
    assert_eq!(full.settings.crouch_height, 72.0);
    assert_eq!(full.settings.step_height, 36.0);
}

#[test]
fn downward_stair_crossing_stays_supported_without_a_large_drop() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0, 96.0, 128.0));
    physics.add_static(&floor(192.0, 72.0, 64.0));
    physics.add_static(&floor(320.0, 48.0, 64.0));
    physics.add_static(&floor(448.0, 24.0, 64.0));
    physics.add_static(&floor(576.0, 0.0, 64.0));

    let mut grounding = NpcGrounding::default();
    grounding
        .update(
            &mut physics,
            ACTOR_A,
            Some(CELL_A),
            [0.0, 0.0, 96.0],
            1.0,
            0.0,
        )
        .unwrap();
    let mut position = [0.0, 0.0, 96.0];
    for _ in 0..300 {
        position = grounding
            .update(
                &mut physics,
                ACTOR_A,
                Some(CELL_A),
                [position[0] + 110.0 / 60.0, 0.0, position[2]],
                1.0,
                1.0 / 60.0,
            )
            .unwrap();
        assert!(position[2] >= -32.0, "fell through a stair: {position:?}");
    }
    assert!(
        position[0] > 500.0,
        "did not cross the stairs: {position:?}"
    );
    assert!(grounding.character(&physics, ACTOR_A).unwrap().on_ground);
    assert!(
        position[2] < 8.0,
        "did not descend to the lower floor: {position:?}"
    );
}

#[test]
fn removing_one_npc_body_does_not_remove_another_actors_controller() {
    let mut physics = SimpleWorld::new();
    physics.add_static(&floor(0.0, 0.0, 512.0));
    let mut grounding = NpcGrounding::default();
    grounding
        .update(
            &mut physics,
            ACTOR_A,
            Some(CELL_A),
            [0.0, 0.0, 80.0],
            1.0,
            0.0,
        )
        .unwrap();
    grounding
        .update(
            &mut physics,
            ACTOR_B,
            Some(CELL_B),
            [100.0, 0.0, 80.0],
            0.75,
            0.0,
        )
        .unwrap();

    grounding.retain(&mut physics, &BTreeSet::from([ACTOR_B]));
    assert!(grounding.character(&physics, ACTOR_A).is_none());
    grounding
        .update(
            &mut physics,
            ACTOR_B,
            Some(CELL_B),
            [100.0, 0.0, 80.0],
            0.75,
            1.0 / 60.0,
        )
        .unwrap();
    let remaining = grounding.character(&physics, ACTOR_B).unwrap();
    assert_eq!(remaining.settings.radius, 24.0);
    assert!(remaining.on_ground);
    let moved = grounding
        .update(
            &mut physics,
            ACTOR_B,
            Some(CELL_B),
            [120.0, 0.0, 80.0],
            0.75,
            0.1,
        )
        .unwrap();
    assert!(
        moved[0] > 100.0 && moved[2] >= -0.01,
        "remaining NPC changed: {moved:?}"
    );
}
