// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The character controller against geometry whose angles are exact.
//!
//! `synthetic.rs` proves the same routines through real `bhk` bytes, which is
//! the right test for the decoder and the wrong one for the controller: a
//! slope written as a rigid body is a slope whose angle is whatever the
//! quaternion rounds to. Here the triangles are built directly, so a 30 degree
//! ramp is 30 degrees and a wall is vertical to the bit, and a failure points
//! at the controller rather than at the file writer.
//!
//! Positions are reported over time rather than asserted only at the end, so a
//! failure shows the trajectory that produced it.

use soe_nif::types::BlockRef;
use soe_physics::{
    BodyId, CharacterSettings, Collider, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3,
};

const DT: f32 = 1.0 / 60.0;

/// A static collider from triangles already in world space.
fn mesh(name: &str, vertices: Vec<Vec3>, triangles: Vec<[u32; 3]>) -> Collider {
    Collider {
        node: BlockRef(0),
        node_name: name.to_string(),
        layer: Layer::STATIC,
        motion: MotionType::Fixed,
        mass: 0.0,
        transform: soe_physics::Iso::IDENTITY,
        shape: Shape::TriangleMesh {
            vertices,
            triangles,
        },
    }
}

/// A flat square floor at `z`, `half` units out from the origin in x and y.
fn floor(z: f32, half: f32) -> Collider {
    mesh(
        "floor",
        vec![
            Vec3::new(-half, -half, z),
            Vec3::new(half, -half, z),
            Vec3::new(half, half, z),
            Vec3::new(-half, half, z),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

/// A ramp rising along +x at `degrees`, starting at the origin and running
/// `run` units out. Wide enough in y that the capsule cannot walk off it.
fn ramp(degrees: f32, run: f32) -> Collider {
    let rise = run * degrees.to_radians().tan();
    let w = 400.0;
    mesh(
        "ramp",
        vec![
            Vec3::new(0.0, -w, 0.0),
            Vec3::new(run, -w, rise),
            Vec3::new(run, w, rise),
            Vec3::new(0.0, w, 0.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

/// A vertical wall across the x axis at `x`.
fn wall(x: f32) -> Collider {
    let (w, h) = (400.0, 400.0);
    mesh(
        "wall",
        vec![
            Vec3::new(x, -w, -10.0),
            Vec3::new(x, w, -10.0),
            Vec3::new(x, w, h),
            Vec3::new(x, -w, h),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

/// Steps and prints the trajectory, returning the positions.
fn run(world: &mut SimpleWorld, id: BodyId, steps: usize, label: &str) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(steps);
    for i in 0..steps {
        world.step(DT);
        let ch = world.character(id).unwrap();
        out.push(ch.position);
        if i % 30 == 0 || i + 1 == steps {
            eprintln!(
                "{label} t={:.2}s pos=({:.1}, {:.1}, {:.1}) ground={}",
                (i + 1) as f32 * DT,
                ch.position.x,
                ch.position.y,
                ch.position.z,
                ch.on_ground
            );
        }
    }
    out
}

#[test]
fn a_capsule_falls_under_gravity_and_lands_on_the_floor() {
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 500.0));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(0.0, 0.0, 400.0));

    let track = run(&mut world, id, 120, "fall");

    // It must actually fall, not teleport or stick.
    assert!(track[0].z < 400.0, "did not start falling: {:?}", track[0]);
    let landed = world.character(id).unwrap();
    assert!(landed.on_ground, "never landed: {landed:?}");
    assert!(
        landed.position.z.abs() < 1.0,
        "feet should rest on the floor plane, got {}",
        landed.position.z
    );
    // Once landed it must stay landed rather than oscillating.
    let tail = &track[track.len() - 20..];
    for p in tail {
        assert!(
            (p.z - landed.position.z).abs() < 0.01,
            "still moving at rest: {p:?} against {:?}",
            landed.position
        );
    }
}

#[test]
fn a_capsule_slides_down_a_slope_steeper_than_it_can_stand_on() {
    // The default settings call anything under about 45 degrees ground.
    let settings = CharacterSettings::default();
    let stand = settings.max_slope_cos.acos().to_degrees();
    assert!(
        (40.0..50.0).contains(&stand),
        "the standable limit moved, this test picks its angles around it: {stand}"
    );

    let mut world = SimpleWorld::new();
    world.add_static(&ramp(60.0, 2000.0));
    let id = world.create_character(settings, Vec3::new(600.0, 0.0, 1400.0));

    let track = run(&mut world, id, 180, "slide");

    let end = world.character(id).unwrap();
    // Down the ramp is negative x, since the surface rises with x.
    assert!(
        end.position.x < 600.0 - 50.0,
        "a 60 degree face should not hold the capsule, it moved to x {}",
        end.position.x
    );
    assert!(
        !end.on_ground,
        "60 degrees is steeper than the standable limit, so it is not ground: {end:?}"
    );
    // Sliding, not tunnelling: it stayed on the outside of the ramp the whole way.
    for p in &track {
        let surface = p.x.max(0.0) * 60.0f32.to_radians().tan();
        assert!(
            p.z > surface - 5.0,
            "sank into the ramp at {p:?}, surface there is {surface}"
        );
    }
}

#[test]
fn a_capsule_walks_up_a_slope_it_can_stand_on() {
    let mut world = SimpleWorld::new();
    world.add_static(&ramp(20.0, 3000.0));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(200.0, 0.0, 200.0));
    // Settle first, then walk up the incline.
    run(&mut world, id, 60, "settle");
    let start = world.character(id).unwrap().position;
    world.move_character(id, Vec3::new(200.0, 0.0, 0.0));

    run(&mut world, id, 120, "climb");

    let end = world.character(id).unwrap();
    assert!(end.on_ground, "20 degrees is walkable ground: {end:?}");
    assert!(
        end.position.x > start.x + 100.0,
        "did not advance up the ramp, {} to {}",
        start.x,
        end.position.x
    );
    assert!(
        end.position.z > start.z + 30.0,
        "advanced without rising, so it cut through the ramp: {} to {}",
        start.z,
        end.position.z
    );
}

#[test]
fn a_capsule_stops_against_a_wall_instead_of_passing_through_it() {
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    world.add_static(&wall(300.0));
    let settings = CharacterSettings::default();
    let id = world.create_character(settings, Vec3::new(0.0, 0.0, 60.0));
    run(&mut world, id, 60, "settle");
    // Straight at the wall, fast enough that a missed contact would show.
    world.move_character(id, Vec3::new(600.0, 0.0, 0.0));

    let track = run(&mut world, id, 180, "push");

    let end = world.character(id).unwrap();
    assert!(
        end.position.x < 300.0,
        "walked through the wall to x {}",
        end.position.x
    );
    assert!(
        end.position.x > 300.0 - settings.radius - 5.0,
        "stopped far short of the wall at x {}, expected to touch near {}",
        end.position.x,
        300.0 - settings.radius
    );
    assert!(end.on_ground, "lost the floor while pushing: {end:?}");
    // No frame may cross the plane, not just the last one.
    for p in &track {
        assert!(p.x < 300.0, "crossed the wall plane at {p:?}");
    }
}

#[test]
fn a_ray_finds_the_floor_under_the_feet() {
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 500.0));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(0.0, 0.0, 300.0));
    run(&mut world, id, 120, "settle");

    let from = world.character(id).unwrap().position + Vec3::Z * 10.0;
    let hit = world
        .raycast(from, from - Vec3::Z * 100.0)
        .expect("a ray down from the feet hits the floor");
    assert!(
        hit.point.z.abs() < 1.0,
        "hit at {:?}, expected the floor plane",
        hit.point
    );
    assert!(
        hit.normal.z > 0.9,
        "floor normal points up, got {:?}",
        hit.normal
    );
}
