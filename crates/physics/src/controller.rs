// SPDX-License-Identifier: GPL-3.0-or-later
// Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab, crates/engine/src/physics.rs at 268f988 and PR 232), MIT License, Copyright (c) the Mudcrab contributors

//! Mudcrab PR 232 walking integration, adapted to Z-up engine capsules.
//! Collision feedback is from the previous controller update, before movement.

use crate::{Character, Vec3};

pub(crate) fn integrate_walk(state: &mut Character, dt: f32, grounded: bool) {
    // CONTEXT: A proximity probe can remain true after takeoff. Positive jump
    // velocity survives that feedback until ascent ends, as in Mudcrab PR 232.
    let grounded = grounded && !(state.jump_active && state.vertical_velocity > 0.0);
    state.on_ground = grounded;
    let wish = Vec3::new(state.desired_velocity.x, state.desired_velocity.y, 0.0);
    let current = state.horizontal_velocity;
    let max_delta = state.settings.horizontal_acceleration * dt;
    let delta = wish - current;
    let horizontal = if delta.length() <= max_delta {
        wish
    } else {
        current + delta.normalized() * max_delta
    };
    state.velocity.x = horizontal.x;
    state.velocity.y = horizontal.y;
    state.horizontal_velocity = horizontal;
    let jump_pressed = std::mem::take(&mut state.jump_requested);
    if grounded {
        state.vertical_velocity = 0.0;
        state.jump_active = false;
        if jump_pressed {
            state.vertical_velocity = state.settings.jump_speed();
            state.jump_active = true;
        }
    } else {
        state.vertical_velocity -= state.settings.gravity * dt;
    }
    state.velocity.z = state.vertical_velocity;
}

#[cfg(test)]
mod ground_probe {
    use super::*;
    use crate::{
        CharacterSettings, Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld,
    };

    const DT: f32 = 1.0 / 60.0;

    fn state() -> Character {
        let mut world = SimpleWorld::new();
        let id = world.create_character(CharacterSettings::mudcrab(), Vec3::ZERO);
        *world.character(id).unwrap()
    }

    #[test]
    fn jump_ascent_survives_ground_probe_and_lands_on_descent() {
        let mut state = state();
        state.jump_requested = true;
        integrate_walk(&mut state, DT, true);
        for step in 1..=10 {
            integrate_walk(&mut state, DT, true);
            assert!(state.jump_active);
            assert!(!state.on_ground);
            assert!((state.velocity.z - (340.0 - 900.0 * DT * step as f32)).abs() < 0.01);
        }
        let before = state.velocity.z;
        state.jump_requested = true;
        integrate_walk(&mut state, DT, true);
        assert!(state.velocity.z < before);
        while state.velocity.z > 0.0 {
            integrate_walk(&mut state, DT, false);
        }
        integrate_walk(&mut state, DT, true);
        assert!(state.on_ground);
        assert!(!state.jump_active);
        assert_eq!(state.velocity.z, 0.0);
    }

    #[test]
    fn jump_requires_grounded_fresh_press_and_applies_gravity() {
        let mut state = state();
        state.jump_requested = true;
        integrate_walk(&mut state, DT, false);
        assert_eq!(state.velocity.z, -900.0 * DT);
        integrate_walk(&mut state, DT, true);
        assert_eq!(state.velocity.z, 0.0, "airborne press is consumed");
        state.jump_requested = true;
        integrate_walk(&mut state, DT, true);
        assert!((state.velocity.z - 340.0).abs() < 0.01);
        integrate_walk(&mut state, DT, false);
        assert!((state.velocity.z - 325.0).abs() < 0.01);
    }

    #[test]
    fn collision_output_does_not_cancel_the_integrated_ascent_request() {
        let mut state = state();
        state.jump_requested = true;
        integrate_walk(&mut state, DT, true);
        state.velocity = Vec3::ZERO;
        integrate_walk(&mut state, DT, true);
        assert!(!state.on_ground);
        assert!(state.jump_active);
        assert!((state.velocity.z - 325.0).abs() < 0.01);
    }

    #[test]
    fn horizontal_acceleration_clamps_to_wish_without_overshoot() {
        let mut state = state();
        state.desired_velocity = Vec3::new(160.0, 0.0, 1000.0);
        let mut expected = 0.0_f32;
        for _ in 1..=6 {
            expected = (expected + 1800.0 * DT).min(160.0);
            integrate_walk(&mut state, DT, true);
            assert_eq!(state.velocity.x, expected);
            assert_eq!(
                state.velocity.z, 0.0,
                "horizontal intent cannot lift the body"
            );
        }
    }

    fn collider(shape: Shape) -> Collider {
        Collider {
            node: soe_nif::types::BlockRef(0),
            node_name: "controller fixture".into(),
            layer: Layer::STATIC,
            motion: MotionType::Fixed,
            mass: 0.0,
            transform: Iso::IDENTITY,
            shape,
        }
    }

    fn floor() -> Collider {
        let mut floor = collider(Shape::Box {
            half_extents: Vec3::new(2000.0, 400.0, 1.0),
        });
        floor.transform = Iso::placement([0.0, 0.0, -1.0], [0.0; 3], 1.0);
        floor
    }

    #[test]
    fn jump_launches_and_returns_to_ground() {
        let mut world = SimpleWorld::new();
        world.add_static(&floor());
        let id = world.create_character(CharacterSettings::mudcrab(), Vec3::Z * 40.0);
        for _ in 0..240 {
            world.step_character(id, DT);
        }
        let rest = world.character(id).unwrap().position.z;
        assert!(world.character(id).unwrap().on_ground);
        assert!(world.try_jump(id));
        assert!(!world.try_jump(id), "one fresh press cannot relaunch");
        let mut apex = rest;
        for _ in 0..180 {
            world.step_character(id, DT);
            let body = world.character(id).unwrap();
            apex = apex.max(body.position.z);
            assert!(body.position.z >= -0.01, "jump cannot cross its floor");
        }
        assert!(apex - rest > 50.0, "Mudcrab launch must survive the probe");
        let landed = world.character(id).unwrap();
        assert!(landed.on_ground);
        assert!((landed.position.z - rest).abs() < 0.01);
    }

    #[test]
    fn autostep_climbs_a_supported_step_and_rejects_a_tall_riser() {
        for (rise, climbs) in [(24.0, true), (72.0, false)] {
            let mut world = SimpleWorld::new();
            world.add_static(&floor());
            let mut step = collider(Shape::Box {
                half_extents: Vec3::new(300.0, 200.0, rise * 0.5),
            });
            step.transform = Iso::placement([400.0, 0.0, rise * 0.5], [0.0; 3], 1.0);
            world.add_static(&step);
            let id = world.create_character(CharacterSettings::mudcrab(), Vec3::ZERO);
            for _ in 0..60 {
                world.step_character(id, DT);
            }
            world.walk(id, Vec3::new(1.0, 0.0, 0.0), 160.0);
            for _ in 0..180 {
                world.step_character(id, DT);
                let body = world.character(id).unwrap();
                assert!(body.position.z >= -0.01, "step {rise}: {body:?}");
            }
            let body = world.character(id).unwrap();
            if climbs {
                assert!(
                    body.position.x > 300.0 && (body.position.z - rise).abs() < 0.01,
                    "supported step: {body:?}"
                );
                assert!(body.on_ground);
            } else {
                assert!(body.position.x <= 70.01 && body.position.z < rise);
            }
        }
    }

    #[test]
    fn slope_climb_and_slide_limits_match_mudcrab() {
        for (degrees, walkable) in [(45.0_f32, true), (52.0, false), (60.0, false)] {
            let mut world = SimpleWorld::new();
            let rise = 2000.0 * degrees.to_radians().tan();
            world.add_static(&collider(Shape::TriangleMesh {
                radius: 0.0,
                vertices: vec![
                    Vec3::new(0.0, -400.0, 0.0),
                    Vec3::new(2000.0, -400.0, rise),
                    Vec3::new(2000.0, 400.0, rise),
                    Vec3::new(0.0, 400.0, 0.0),
                ],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                materials: Vec::new(),
            }));
            let start = Vec3::new(500.0, 0.0, 500.0 * degrees.to_radians().tan() + 50.0);
            let id = world.create_character(CharacterSettings::mudcrab(), start);
            for _ in 0..120 {
                world.step_character(id, DT);
            }
            if degrees == 52.0 {
                let before = world.character(id).unwrap().position;
                for _ in 0..120 {
                    world.step_character(id, DT);
                }
                let body = world.character(id).unwrap();
                assert!(!body.on_ground);
                assert!(
                    (body.position - before).length() < 0.01,
                    "slide threshold: {body:?}"
                );
            } else if walkable {
                let before = world.character(id).unwrap().position;
                world.walk(id, Vec3::new(1.0, 0.0, 0.0), 160.0);
                for _ in 0..120 {
                    world.step_character(id, DT);
                }
                let body = world.character(id).unwrap();
                assert!(
                    body.on_ground && body.position.x > before.x + 100.0,
                    "climb from {before:?}: {body:?}"
                );
            } else {
                let body = world.character(id).unwrap();
                assert!(!body.on_ground && body.position.x < start.x - 50.0);
                let before = body.position;
                world.walk(id, Vec3::new(1.0, 0.0, 0.0), 160.0);
                for _ in 0..120 {
                    world.step_character(id, DT);
                }
                assert!(world.character(id).unwrap().position.x <= before.x + 0.01);
            }
        }
    }
}
