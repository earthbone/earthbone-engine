// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics sight queries feeding the shared detection updater.
//! PhysicsDirectSpaceState3D.intersect_ray reports the first body RID.

use std::collections::BTreeMap;

use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::detection::Motion;

use super::PlayerNode;
use crate::godot_combat::{first_contact, ContactHit};
use crate::units::SkyrimPos;

#[godot_api(secondary)]
impl PlayerNode {
    /// Run one shared detection sample on demand. The gameplay loop calls the
    /// same path each physics step; this entry point lets host checks sample
    /// the live physics space at a controlled point in time.
    #[func]
    pub fn sample_detection(&mut self, elapsed: f64) -> i64 {
        self.advance_detection(elapsed, Motion::Still)
    }

    /// Read one observer's shared detection value for a host integration check.
    #[func]
    pub fn detection_value(
        &self,
        observer_file: i32,
        observer_object: i64,
        target_file: i32,
        target_object: i64,
    ) -> f64 {
        let (Ok(observer_file), Ok(observer_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(observer_file),
            u32::try_from(observer_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return -1.0;
        };
        let observer = GlobalFormId {
            file: observer_file,
            object: observer_object,
        };
        let target = GlobalFormId {
            file: target_file,
            object: target_object,
        };
        self.game
            .as_ref()
            .and_then(|game| game.world().awareness(observer, target))
            .map_or(-1.0, |value| f64::from(value.detection))
    }

    /// Whether the shared detection state has an observer entry for this pair.
    #[func]
    pub fn has_detection_awareness(
        &self,
        observer_file: i32,
        observer_object: i64,
        target_file: i32,
        target_object: i64,
    ) -> bool {
        let (Ok(observer_file), Ok(observer_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(observer_file),
            u32::try_from(observer_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return false;
        };
        let observer = GlobalFormId {
            file: observer_file,
            object: observer_object,
        };
        let target = GlobalFormId {
            file: target_file,
            object: target_object,
        };
        self.game
            .as_ref()
            .is_some_and(|game| game.world().awareness(observer, target).is_some())
    }

    /// Queue a sound from an actor reference for the next shared detection step.
    #[func]
    pub fn emit_detection_sound(&mut self, file: i32, object: i64, sound_level: i32) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let owner = GlobalFormId { file, object };
        self.game.as_mut().is_some_and(|game| {
            game.world_mut()
                .create_detection_event(owner, owner, sound_level)
        })
    }

    pub(super) fn advance_detection(&mut self, elapsed: f64, player_motion: Motion) -> i64 {
        if !elapsed.is_finite() || elapsed < 0.0 || !self.sync_actor_bodies() {
            return -1;
        }
        let Some(space) = self.base().get_world_3d().map(|world| world.get_space()) else {
            return -1;
        };
        let Some(game) = &mut self.game else {
            return -1;
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
        let pending_events = game.world_mut().take_detection_events();
        let events = pending_events
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
        let observation_count = observations.len() as i64;
        if let Err(error) = game
            .world_mut()
            .evaluate_detection(&observations, elapsed as f32)
        {
            godot_error!("PlayerNode detection: {error}");
            -1
        } else {
            observation_count
        }
    }
}
