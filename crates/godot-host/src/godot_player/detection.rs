// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics sight queries feeding the shared detection updater.
//! PhysicsDirectSpaceState3D.intersect_ray reports the first body RID.

use std::collections::BTreeMap;

use godot::prelude::*;
use soe_game::detection::Motion;

use super::PlayerNode;
use crate::godot_combat::{first_contact, ContactHit};
use crate::units::SkyrimPos;

impl PlayerNode {
    pub(super) fn advance_detection(&mut self, elapsed: f64, player_motion: Motion) {
        if !elapsed.is_finite() || elapsed < 0.0 || !self.sync_actor_bodies() {
            return;
        }
        let Some(space) = self.base().get_world_3d().map(|world| world.get_space()) else {
            return;
        };
        let Some(game) = &mut self.game else {
            return;
        };
        let player = game.player();
        let mut actors = game.world_mut().contact_actors();
        actors.retain(|actor| {
            game.world_mut()
                .actor(actor.actor)
                .is_some_and(|state| !state.dead)
        });
        let mut motions = BTreeMap::new();
        motions.insert(player, player_motion);
        let events = game
            .world_mut()
            .take_detection_events()
            .into_iter()
            .filter(|event| {
                game.world_mut()
                    .actor(event.owner)
                    .is_some_and(|state| !state.dead)
            })
            .collect::<Vec<_>>();
        let owners = [&self.melee_bodies, self.actor_bodies.owners()];
        let world = game.world();
        let observations = crate::detection::observations(
            &actors,
            &events,
            &motions,
            |observer, target| {
                let position = |actor: &soe_game::world::ContactActor| {
                    let p = actor.position;
                    let eye = SkyrimPos::new(p[0], p[1], p[2] + 100.0 * actor.scale).to_godot();
                    Vector3::new(eye.x, eye.y, eye.z)
                };
                match first_contact(
                    space,
                    &owners,
                    observer.actor,
                    position(observer),
                    position(target),
                ) {
                    Ok(ContactHit::Empty) => true,
                    Ok(ContactHit::Actor(id)) => id == target.actor,
                    _ => false,
                }
            },
            |cell, point| crate::detection::light_level(&world, cell, point),
        );
        drop(world);
        if let Err(error) = game
            .world_mut()
            .evaluate_detection(&observations, elapsed as f32)
        {
            godot_error!("PlayerNode detection: {error}");
        }
    }
}
