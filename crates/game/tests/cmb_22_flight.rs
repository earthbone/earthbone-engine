// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-22 deterministic arrow and bolt motion.

use soe_esm::GlobalFormId;
use soe_game::combat::{FlightMotion, FlightRequest};

fn request(gravity: f32) -> FlightRequest {
    FlightRequest {
        attacker: GlobalFormId { file: 0, object: 1 },
        weapon: GlobalFormId { file: 0, object: 2 },
        ammunition: GlobalFormId { file: 0, object: 3 },
        projectile: GlobalFormId { file: 0, object: 4 },
        origin: [0.0; 3],
        direction: [1.0, 0.0, 0.0],
        gravity,
        speed: 100.0,
        range: 1000.0,
        damage: 1.0,
    }
}

#[test]
fn cmb_22_configured_gravity_bends_flight_by_the_record_value() {
    let mut gravity = FlightMotion::new(request(0.1));
    let mut weightless = FlightMotion::new(request(0.0));
    let gravity_segment = gravity.advance(1.0).unwrap().unwrap();
    let weightless_segment = weightless.advance(1.0).unwrap().unwrap();
    assert!((gravity_segment.to[2] + 34.335).abs() < 0.001);
    assert_eq!(weightless_segment.to[2], 0.0);
}

#[test]
fn cmb_22_fixed_steps_produce_identical_flight_segments() {
    let mut first = FlightMotion::new(request(0.1));
    let mut second = FlightMotion::new(request(0.1));
    for _ in 0..8 {
        assert_eq!(
            first.advance(1.0 / 60.0).unwrap(),
            second.advance(1.0 / 60.0).unwrap()
        );
    }
}

#[test]
fn cmb_22_projectile_stops_at_configured_range() {
    let mut flight = FlightMotion::new(request(0.0));
    let segment = flight.advance(20.0).unwrap().unwrap();
    assert_eq!(segment.to, [1000.0, 0.0, 0.0]);
    assert!(!flight.is_active());
    assert!(flight.advance(1.0).unwrap().is_none());
}
