// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Translation survey for the former Skyrim physics boundary.
//! Godot's engine-refs/godot-docs/tutorials/physics/large_world_coordinates.rst
//! explains why distance from the origin needs a measured tolerance.

use soe_nif::types::BlockRef;
use soe_physics::{
    CharacterSettings, Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3,
    JOG_SPEED,
};

const CELL: f32 = 4096.0;
const DT: f32 = 1.0 / 60.0;
const CONTACT_ERROR: f32 = 1.0;
const TRAVEL_ERROR: f32 = 5.0;

fn offsets() -> Vec<Vec3> {
    let mut out = vec![Vec3::ZERO];
    for cell in [64.0, 65.0, 128.0, 256.0] {
        for (x, y) in [
            (1.0, 0.0),
            (-1.0, 0.0),
            (0.0, 1.0),
            (0.0, -1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (1.0, -1.0),
            (-1.0, -1.0),
        ] {
            out.push(Vec3::new(x * cell * CELL, y * cell * CELL, 0.0));
        }
    }
    out
}

fn mesh(offset: Vec3, vertices: Vec<Vec3>) -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: "large world fixture".into(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: Iso::translation(offset),
        shape: Shape::TriangleMesh {
            vertices,
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        },
    }
}

fn floor(offset: Vec3) -> Collider {
    mesh(
        offset,
        vec![
            Vec3::new(-800.0, -800.0, 0.0),
            Vec3::new(800.0, -800.0, 0.0),
            Vec3::new(800.0, 800.0, 0.0),
            Vec3::new(-800.0, 800.0, 0.0),
        ],
    )
}

fn wall(offset: Vec3, direction: Vec3) -> Collider {
    let side = Vec3::new(-direction.y, direction.x, 0.0) * 400.0;
    let centre = direction * 300.0;
    mesh(
        offset,
        vec![
            centre - side,
            centre + side,
            centre + side + Vec3::Z * 400.0,
            centre - side + Vec3::Z * 400.0,
        ],
    )
}

#[test]
fn falling_and_walking_work_on_axes_and_diagonals_beyond_64_cells() {
    let mut max_travel_error = 0.0_f32;
    for offset in offsets() {
        for direction in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(-1.0, -1.0, 0.0),
        ] {
            let mut world = SimpleWorld::new();
            world.add_static(&floor(offset));
            let id = world.create_character(CharacterSettings::default(), offset + Vec3::Z * 400.0);
            for _ in 0..120 {
                world.step(DT);
            }
            let start = world.character(id).unwrap().position;
            assert!(
                world.character(id).unwrap().on_ground,
                "not grounded at {offset:?}"
            );
            assert!(
                start.z.abs() <= CONTACT_ERROR,
                "floor missed at {offset:?}: {start:?}"
            );
            world.walk(id, direction, JOG_SPEED);
            for _ in 0..60 {
                world.step(DT);
                let ch = world.character(id).unwrap();
                assert!(
                    ch.on_ground && ch.position.z.abs() <= CONTACT_ERROR,
                    "lost contact at {offset:?}, direction {direction:?}: {ch:?}"
                );
            }
            let travelled = world.character(id).unwrap().position - start;
            let error = (travelled - direction.normalized() * JOG_SPEED).length();
            max_travel_error = max_travel_error.max(error);
            assert!(
                error <= TRAVEL_ERROR,
                "travel error {error} at {offset:?}, direction {direction:?}"
            );
        }
    }
    eprintln!("198 walks at 33 placements, maximum travel error {max_travel_error} game units");
}

#[test]
fn walls_block_and_rays_hit_facing_toward_and_away_from_the_origin() {
    for offset in offsets() {
        for direction in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ] {
            let mut world = SimpleWorld::new();
            world.add_static(&floor(offset));
            let wall_id = world.add_static(&wall(offset, direction));
            let eye = offset + Vec3::Z * 100.0;
            let hit = world
                .raycast(eye, eye + direction * 500.0)
                .unwrap_or_else(|| panic!("ray missed at {offset:?}, direction {direction:?}"));
            assert_eq!(hit.body, wall_id, "wrong target at {offset:?}");
            assert!(
                (hit.point - (eye + direction * 300.0)).length() <= CONTACT_ERROR,
                "wrong hit point at {offset:?}: {hit:?}"
            );
            let settings = CharacterSettings::default();
            let id = world.create_character(settings, offset + Vec3::Z * 40.0);
            for _ in 0..60 {
                world.step(DT);
            }
            world.walk(id, direction, 600.0);
            for _ in 0..120 {
                world.step(DT);
                let distance = (world.character(id).unwrap().position - offset).dot(direction);
                assert!(
                    distance <= 300.0 - settings.radius + CONTACT_ERROR,
                    "crossed wall at {offset:?}, direction {direction:?}: {distance}"
                );
            }
            let ch = world.character(id).unwrap();
            let distance = (ch.position - offset).dot(direction);
            assert!(
                (distance - (300.0 - settings.radius)).abs() <= CONTACT_ERROR,
                "stopped short at {offset:?}, direction {direction:?}: {distance}"
            );
            assert!(ch.on_ground, "lost ground at {offset:?}");
            world.remove(wall_id);
            assert!(
                world.raycast(eye, eye + direction * 500.0).is_none(),
                "removed wall still hit at {offset:?}"
            );
        }
    }
}
