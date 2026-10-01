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
            materials: Vec::new(),
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

/// A flat ceiling at `z`, spanning x from `x0` to `x1` and wide in y, so a
/// capsule can walk out from under it.
fn ceiling(z: f32, x0: f32, x1: f32) -> Collider {
    let w = 400.0;
    mesh(
        "ceiling",
        vec![
            Vec3::new(x0, -w, z),
            Vec3::new(x1, -w, z),
            Vec3::new(x1, w, z),
            Vec3::new(x0, w, z),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    )
}

/// A flight of steps climbing along +x from the origin, each `rise` tall and
/// `run` deep. Both the tread and the riser are present, so a capsule that
/// cannot climb is stopped by the riser rather than passing through it.
fn stairs(count: usize, rise: f32, run: f32) -> Collider {
    let w = 400.0;
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
        let base = u32::try_from(vertices.len()).expect("fixture stays small");
        vertices.extend_from_slice(&[a, b, c, d]);
        triangles.push([base, base + 1, base + 2]);
        triangles.push([base, base + 2, base + 3]);
    };
    for i in 0..count {
        let (x0, x1) = (i as f32 * run, (i + 1) as f32 * run);
        let (z0, z1) = (i as f32 * rise, (i + 1) as f32 * rise);
        // The riser, a vertical face at the front of the step.
        quad(
            Vec3::new(x0, -w, z0),
            Vec3::new(x0, w, z0),
            Vec3::new(x0, w, z1),
            Vec3::new(x0, -w, z1),
        );
        // The tread, the flat top walked on.
        quad(
            Vec3::new(x0, -w, z1),
            Vec3::new(x1, -w, z1),
            Vec3::new(x1, w, z1),
            Vec3::new(x0, w, z1),
        );
    }
    mesh("stairs", vertices, triangles)
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
fn a_capsule_climbs_a_stair_riser_shorter_than_its_step_height() {
    let settings = CharacterSettings::default();
    let rise = 24.0;
    assert!(
        rise < settings.step_height,
        "this test wants a riser the capsule can clear, {rise} against {}",
        settings.step_height
    );

    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    world.add_static(&stairs(8, rise, 48.0));
    let id = world.create_character(settings, Vec3::new(-200.0, 0.0, 40.0));
    run(&mut world, id, 60, "settle");
    let start = world.character(id).unwrap().position;
    world.walk(id, Vec3::new(1.0, 0.0, 0.0), soe_physics::WALK_SPEED);

    let track = run(&mut world, id, 300, "stairs");

    let end = world.character(id).unwrap();
    assert!(end.on_ground, "lost the stairs while climbing: {end:?}");
    assert!(
        end.position.z > start.z + 3.0 * rise,
        "climbed fewer than three steps, {} to {}",
        start.z,
        end.position.z
    );
    assert!(
        end.position.x > 3.0 * 48.0,
        "did not advance along the flight, x {}",
        end.position.x
    );
    // Climbing, not floating. The step up lifts before the capsule is over
    // the next tread, so the bar is the tread a radius ahead of the feet.
    let tread_at = |x: f32| -> f32 {
        if x <= 0.0 {
            0.0
        } else {
            ((x / 48.0).floor() + 1.0).min(8.0) * rise
        }
    };
    for p in &track {
        let bar = tread_at(p.x + settings.radius);
        assert!(
            p.z <= bar + 2.0,
            "rose above the step ahead at {p:?}, which tops out at {bar}"
        );
    }
}

#[test]
fn a_riser_taller_than_the_step_height_stops_the_capsule() {
    let settings = CharacterSettings::default();
    let rise = 72.0;
    assert!(
        rise > settings.step_height,
        "this test wants an unclimbable riser"
    );

    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    world.add_static(&stairs(4, rise, 96.0));
    let id = world.create_character(settings, Vec3::new(-200.0, 0.0, 40.0));
    run(&mut world, id, 60, "settle");
    world.walk(id, Vec3::new(1.0, 0.0, 0.0), soe_physics::RUN_SPEED);

    run(&mut world, id, 180, "blocked");

    let end = world.character(id).unwrap();
    assert!(
        end.position.z < rise,
        "cleared a riser taller than the step height, z {}",
        end.position.z
    );
    assert!(
        end.position.x < settings.radius + 1.0,
        "passed through the first riser to x {}",
        end.position.x
    );
}

#[test]
fn a_capsule_steps_over_a_door_threshold() {
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    // One shallow step, which is the shape of a threshold under a door.
    world.add_static(&stairs(1, 12.0, 24.0));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(-200.0, 0.0, 40.0));
    run(&mut world, id, 60, "settle");
    world.walk(id, Vec3::new(1.0, 0.0, 0.0), soe_physics::WALK_SPEED);

    run(&mut world, id, 180, "threshold");

    let end = world.character(id).unwrap();
    assert!(
        end.position.x > 24.0,
        "stopped at the threshold instead of crossing it, x {}",
        end.position.x
    );
    assert!(
        end.on_ground,
        "left the ground crossing a threshold: {end:?}"
    );
}

#[test]
fn walking_off_a_riser_descends_without_a_sudden_drop() {
    let mut world = SimpleWorld::new();
    // A raised slab that ends at x 0, with the ground one step below it.
    world.add_static(&floor(0.0, 900.0));
    world.add_static(&mesh(
        "slab",
        vec![
            Vec3::new(-600.0, -400.0, 30.0),
            Vec3::new(0.0, -400.0, 30.0),
            Vec3::new(0.0, 400.0, 30.0),
            Vec3::new(-600.0, 400.0, 30.0),
        ],
        vec![[0, 1, 2], [0, 2, 3]],
    ));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(-300.0, 0.0, 80.0));
    run(&mut world, id, 60, "settle");
    assert!((world.character(id).unwrap().position.z - 30.0).abs() < 1.0);
    world.walk(id, Vec3::new(1.0, 0.0, 0.0), soe_physics::WALK_SPEED);

    let track = run(&mut world, id, 240, "stepdown");

    let end = world.character(id).unwrap();
    assert!(
        end.position.x > 0.0,
        "never left the slab, x {}",
        end.position.x
    );
    assert!(end.on_ground, "did not settle after the drop: {end:?}");
    assert!(
        end.position.z.abs() < 1.0,
        "should be on the lower floor, z {}",
        end.position.z
    );
    for pair in track.windows(2) {
        let descent = pair[0].z - pair[1].z;
        assert!(
            descent <= 8.0,
            "sudden drop of {descent} units: {:?} -> {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn a_capsule_cannot_stand_up_under_a_ceiling_too_low_for_it() {
    let settings = CharacterSettings::default();
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    // Low enough for the crouched capsule and not the standing one.
    let z = 110.0;
    assert!(
        settings.crouch_height < z && z < settings.height,
        "the ceiling must sit between the two heights"
    );
    world.add_static(&ceiling(z, -600.0, 0.0));

    // Crouch before the first step, so the capsule never spawns inside the
    // ceiling and get pushed out on the wrong side of it.
    let id = world.create_character(settings, Vec3::new(-300.0, 0.0, 5.0));
    assert!(world.set_crouch(id, true), "crouching is always allowed");
    assert!(world.character(id).unwrap().crouching);
    run(&mut world, id, 60, "crouched");
    assert!(
        world.character(id).unwrap().position.z.abs() < 1.0,
        "should have settled on the floor under the ceiling, not on top of it"
    );

    assert!(
        world.set_crouch(id, false),
        "standing up under the ceiling must be refused, so it stays crouched"
    );
    assert!(
        world.character(id).unwrap().crouching,
        "refused, so it is still crouching"
    );

    // Walk out from under it, then it fits.
    world.walk(id, Vec3::new(1.0, 0.0, 0.0), soe_physics::WALK_SPEED);
    run(&mut world, id, 240, "out");
    world.walk(id, Vec3::ZERO, 0.0);
    let out = world.character(id).unwrap().position;
    assert!(out.x > 100.0, "did not clear the ceiling, x {}", out.x);
    assert!(
        !world.set_crouch(id, false),
        "clear of the ceiling, standing up must be allowed"
    );
    assert!(!world.character(id).unwrap().crouching);
}

#[test]
fn a_jump_leaves_the_ground_reaches_its_height_and_lands() {
    let settings = CharacterSettings::default();
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 900.0));
    let id = world.create_character(settings, Vec3::new(0.0, 0.0, 40.0));
    run(&mut world, id, 60, "settle");
    let ground = world.character(id).unwrap().position.z;

    assert!(world.try_jump(id), "standing, so the jump is taken");
    assert!(
        !world.try_jump(id),
        "already airborne, so a second jump is refused"
    );

    let track = run(&mut world, id, 120, "jump");
    let peak = track.iter().fold(f32::MIN, |m, p| m.max(p.z)) - ground;
    eprintln!(
        "jump peak {peak:.1} units, aiming at {}",
        settings.jump_height
    );
    assert!(
        (settings.jump_height * 0.8..settings.jump_height * 1.2).contains(&peak),
        "peak {peak} is not near the configured {}",
        settings.jump_height
    );

    let end = world.character(id).unwrap();
    assert!(end.on_ground, "never came back down: {end:?}");
    assert!(
        (end.position.z - ground).abs() < 1.0,
        "landed at {}",
        end.position.z
    );
}

#[test]
fn walk_turns_a_direction_and_a_speed_into_movement() {
    let mut world = SimpleWorld::new();
    world.add_static(&floor(0.0, 2000.0));
    let id = world.create_character(CharacterSettings::default(), Vec3::new(0.0, 0.0, 40.0));
    run(&mut world, id, 60, "settle");

    // An unnormalised diagonal must still move at the speed asked for.
    world.walk(id, Vec3::new(3.0, 3.0, 9.0), soe_physics::JOG_SPEED);
    let before = world.character(id).unwrap().position;
    run(&mut world, id, 60, "jog");
    let after = world.character(id).unwrap().position;
    let travelled = ((after.x - before.x).powi(2) + (after.y - before.y).powi(2)).sqrt();
    eprintln!("jogged {travelled:.1} units in one second");
    assert!(
        (travelled - soe_physics::JOG_SPEED).abs() < 5.0,
        "one second at jog speed should cover about {}, covered {travelled}",
        soe_physics::JOG_SPEED
    );
    assert!(
        (after.x - before.x - (after.y - before.y)).abs() < 1.0,
        "a 45 degree direction should move equally in x and y"
    );

    // A zero direction stops rather than coasting.
    world.walk(id, Vec3::ZERO, soe_physics::RUN_SPEED);
    let held = world.character(id).unwrap().position;
    run(&mut world, id, 30, "stop");
    let rest = world.character(id).unwrap().position;
    assert!(
        ((rest.x - held.x).powi(2) + (rest.y - held.y).powi(2)).sqrt() < 1.0,
        "kept moving after being told to stop, {held:?} to {rest:?}"
    );
}

#[test]
fn the_speed_constants_are_the_games_own() {
    // The package driver in soe-game walks actors at these, so they must not
    // drift apart. That crate reads them from here.
    assert_eq!(soe_physics::WALK_SPEED, 110.0);
    assert_eq!(soe_physics::FAST_WALK_SPEED, 160.0);
    assert_eq!(soe_physics::JOG_SPEED, 180.0);
    assert_eq!(soe_physics::RUN_SPEED, 375.0);
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
