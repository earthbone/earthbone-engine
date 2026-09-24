// SPDX-License-Identifier: GPL-3.0-or-later

//! Animation contact entry points into the player's shared Game.
//! The caller supplies the authored weapon segment during a physics tick.

use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::combat::Hand;

use super::PlayerNode;

#[godot_api(secondary)]
impl PlayerNode {
    /// Static body RIDs belonging to the currently loaded controller location.
    /// Diagnostic callers may inspect these handles, but must not free them.
    #[func]
    pub fn contact_collision_bodies(&self) -> Array<Rid> {
        self.query_collision.body_rids().collect()
    }

    /// Shape count for the current query location, for host resource diagnostics.
    #[func]
    pub fn contact_collision_shape_count(&self) -> i64 {
        self.query_collision.stats.shapes as i64
    }

    /// Register an existing actor physics body without allocating a host node.
    /// The collision owner must unregister its RID before freeing it.
    #[func]
    pub fn register_melee_body(&mut self, body: Rid, file: i32, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let Some(game) = &self.game else { return false };
        let form = GlobalFormId { file, object };
        if !body.is_valid() || game.world_mut().actor(form).is_none() {
            return false;
        }
        self.melee_bodies.insert(body, form);
        true
    }

    /// Remove collision ownership before a body is destroyed or replaced.
    #[func]
    pub fn unregister_melee_body(&mut self, body: Rid) {
        self.melee_bodies.remove(&body);
    }

    /// Arm an ordinary swing. Input binding is owned by CMB-04.
    #[func]
    pub fn begin_melee_swing(&mut self, left_hand: bool) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        let player = game.player();
        let hand = if left_hand { Hand::Left } else { Hand::Right };
        match self.melee_contact.begin(game, player, hand) {
            Ok(()) => true,
            Err(error) => {
                godot_error!("PlayerNode: {error}");
                false
            }
        }
    }

    /// Cancel on interruption, including a replaced animation or loaded game.
    #[func]
    pub fn cancel_melee_swing(&mut self) {
        self.melee_contact.cancel();
    }

    /// Apply a graph event and world-space weapon segment during a physics tick.
    /// Returns true only when the event caused one accepted contact.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn melee_animation_contact(&mut self, name: GString, from: Vector3, to: Vector3) -> bool {
        let Some(world) = self.base().get_world_3d() else {
            return false;
        };
        let space = world.get_space();
        let Some(game) = &mut self.game else {
            return false;
        };
        let bodies = &self.melee_bodies;
        let result = self
            .melee_contact
            .event(game, &name.to_string(), |attacker| {
                crate::godot_combat::first_contact(space, bodies, attacker, from, to)
            });
        match result {
            Ok(hit) => hit.is_some(),
            Err(error) => {
                godot_error!("PlayerNode: {error}");
                false
            }
        }
    }
}

impl PlayerNode {
    pub(super) fn prepare_query_collision(
        &self,
        collision: &crate::collision::CellCollision,
    ) -> Option<crate::godot_physics::CellBodies> {
        let space = self.base().get_world_3d()?.get_space();
        if !space.is_valid() {
            return None;
        }
        Some(crate::godot_physics::add_statics(
            space,
            &collision.colliders,
        ))
    }
}
