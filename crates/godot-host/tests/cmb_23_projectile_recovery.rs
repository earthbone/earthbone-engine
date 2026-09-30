// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-23 one-time recovery after a projectile hits the world.

use godot::builtin::Rid;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::FlightRequest, Game};
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
fn cmb_23_world_impact_creates_one_placed_arrow_reference() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"AMMO"), request().ammunition.object));
    let mut order = LoadOrder::new();
    order.add("Cmb23.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut request = request();
    request.attacker = game.player();
    let impact = [15.0, 0.0, 0.0];
    let mut flight = ProjectileFlight::new(request);
    assert_eq!(
        flight
            .advance_with(0.1, |_| {
                Ok(Some(ProjectileContact {
                    hit: ContactHit::Obstruction(Rid::Invalid),
                    position: impact,
                }))
            })
            .unwrap(),
        ProjectileUpdate::Contact(ProjectileContact {
            hit: ContactHit::Obstruction(Rid::Invalid),
            position: impact,
        })
    );
    let arrow = flight.recover_missed_arrow(&mut game).unwrap();
    assert!(flight.recover_missed_arrow(&mut game).is_none());
    let world = game.world_mut();
    let reference = world.reference_ref(arrow).unwrap();
    assert_eq!(reference.base, Some(request.ammunition));
    assert_eq!(reference.position, impact);
}

#[test]
fn cmb_23_actor_contact_does_not_create_a_recoverable_arrow() {
    let mut game = Game::new(soe_esm::LoadOrder::new());
    let mut flight = ProjectileFlight::new(request());
    flight
        .advance_with(0.1, |_| {
            Ok(Some(ProjectileContact {
                hit: ContactHit::Actor(GlobalFormId { file: 0, object: 5 }),
                position: [15.0, 0.0, 0.0],
            }))
        })
        .unwrap();
    assert!(flight.recover_missed_arrow(&mut game).is_none());
}
