// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-22 projectile first-contact behavior.

use std::cell::Cell;

use godot::builtin::Rid;
use soe_esm::GlobalFormId;
use soe_game::combat::FlightRequest;
use soe_godot_host::combat::{ProjectileContact, ProjectileFlight, ProjectileUpdate};
use soe_godot_host::godot_combat::ContactHit;

fn request() -> FlightRequest {
    FlightRequest {
        attacker: GlobalFormId { file: 0, object: 1 },
        weapon: GlobalFormId { file: 0, object: 2 },
        ammunition: GlobalFormId { file: 0, object: 3 },
        projectile: GlobalFormId { file: 0, object: 4 },
        origin: [0.0; 3],
        direction: [1.0, 0.0, 0.0],
        gravity: 0.0,
        speed: 100.0,
        range: 1000.0,
        damage: 1.0,
        recoverable: true,
    }
}

#[test]
fn cmb_22_target_contact_is_resolved_once() {
    let mut flight = ProjectileFlight::new(request());
    let target = GlobalFormId { file: 0, object: 5 };
    let calls = Cell::new(0);
    let contact = ProjectileContact {
        hit: ContactHit::Actor(target),
        position: [25.0, 0.0, 0.0],
    };
    assert_eq!(
        flight
            .advance_with(0.1, |_| {
                calls.set(calls.get() + 1);
                Ok(Some(contact))
            })
            .unwrap(),
        ProjectileUpdate::Contact(contact)
    );
    assert_eq!(
        flight
            .advance_with(0.1, |_| panic!("contact queried twice"))
            .unwrap(),
        ProjectileUpdate::Finished
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn cmb_22_obstruction_contact_is_resolved_once() {
    let mut flight = ProjectileFlight::new(request());
    let contact = ProjectileContact {
        hit: ContactHit::Obstruction(Rid::Invalid),
        position: [15.0, 0.0, 0.0],
    };
    assert_eq!(
        flight.advance_with(0.1, |_| Ok(Some(contact))).unwrap(),
        ProjectileUpdate::Contact(contact)
    );
    assert_eq!(
        flight
            .advance_with(0.1, |_| panic!("contact queried twice"))
            .unwrap(),
        ProjectileUpdate::Finished
    );
}

#[test]
fn cmb_22_clear_flight_stays_active_until_range() {
    let mut flight = ProjectileFlight::new(request());
    let result = flight.advance_with(0.1, |_| Ok(None)).unwrap();
    assert_eq!(
        result,
        ProjectileUpdate::Flying {
            position: [10.0, 0.0, 0.0]
        }
    );
}
