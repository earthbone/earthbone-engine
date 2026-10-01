// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial RND-14 checks for birth rate, visibility and particle expiry.

use soe_nif::controller::{ControllerFlags, CycleType, TimeController};
use soe_nif::particle::{Emitter, Rate, Timeline, Visibility};
use soe_nif::types::Transform;

fn emitter(rate: f32, lifetime: f32, active: bool, visibility: Visibility) -> Emitter {
    Emitter {
        name: "adversarial".into(),
        system_name: "system".into(),
        transform: Transform::IDENTITY,
        rate: Rate {
            time: TimeController {
                flags: ControllerFlags {
                    active,
                    cycle_type: CycleType::Clamp,
                    ..Default::default()
                },
                frequency: 1.0,
                start_time: 0.0,
                stop_time: 100.0,
                ..Default::default()
            },
            value: rate,
            keys: None,
            visibility,
        },
        lifetime,
        lifetime_variation: 0.0,
        radius: 1.0,
        color: [1.0; 4],
        capacity: 128,
    }
}

#[test]
fn rnd_14_inactive_rate_controller_emits_nothing() {
    let mut timeline = Timeline::new(emitter(
        4.0,
        2.0,
        false,
        Visibility {
            default: true,
            keys: Vec::new(),
        },
    ));
    timeline.advance(1.0);
    assert_eq!(timeline.emitted(), 0);
    assert_eq!(timeline.alive(), 0);
}

#[test]
fn rnd_14_zero_rate_does_not_create_particles() {
    let mut timeline = Timeline::new(emitter(
        0.0,
        2.0,
        true,
        Visibility {
            default: true,
            keys: Vec::new(),
        },
    ));
    timeline.advance(10.0);
    assert_eq!(timeline.emitted(), 0);
}

#[test]
fn rnd_14_particles_expire_at_the_lifetime_boundary() {
    let mut timeline = Timeline::new(emitter(
        2.0,
        0.5,
        true,
        Visibility {
            default: true,
            keys: Vec::new(),
        },
    ));
    timeline.advance(0.5);
    assert_eq!(timeline.alive(), 1);
    timeline.advance(0.5);
    assert_eq!(timeline.alive(), 1);
    assert_eq!(timeline.particles()[0].born_at, 1.0);
}

#[test]
fn rnd_14_visibility_off_after_key_stops_future_births() {
    let mut timeline = Timeline::new(emitter(
        4.0,
        10.0,
        true,
        Visibility {
            default: true,
            keys: vec![(0.5, false)],
        },
    ));
    timeline.advance(1.0);
    assert_eq!(timeline.emitted(), 1);
    assert_eq!(timeline.particles()[0].born_at, 0.25);
}

#[test]
fn rnd_14_visibility_off_key_excludes_birth_at_its_boundary() {
    let mut timeline = Timeline::new(emitter(
        4.0,
        10.0,
        true,
        Visibility {
            default: true,
            keys: vec![(0.5, false)],
        },
    ));
    timeline.advance(1.0);
    assert_eq!(timeline.emitted(), 1);
    assert_eq!(timeline.particles()[0].born_at, 0.25);
}
