// SPDX-License-Identifier: GPL-3.0-or-later

//! Animation contact entry points into the player's shared Game.
//! The caller supplies the authored weapon segment during a physics tick.

use godot::classes::Skeleton3D;
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
        self.query_collision.bodies.iter().copied().collect()
    }

    /// Shape count for the current query location, for host resource diagnostics.
    #[func]
    pub fn contact_collision_shape_count(&self) -> i64 {
        self.query_collision.shapes.len() as i64
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
        self.contact_event(&name.to_string(), |_, _, _| Ok((from, to)))
    }

    /// Apply a graph event with the weapon segment derived from the armed
    /// hand's weapon bone in `skeleton`'s current pose. Reach comes from the
    /// shared Game: `fCombatDistance` times reference scale times the
    /// weapon's reach. Returns true only when the event caused one contact.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn melee_pose_contact(&mut self, name: GString, skeleton: Gd<Skeleton3D>) -> bool {
        self.contact_event(&name.to_string(), |game, attacker, hand| {
            let reach = game.world_mut().melee_reach(attacker, hand)?;
            let pose = crate::godot_combat::weapon_bone_pose(&skeleton, hand)?;
            crate::godot_combat::pose_segment(pose, reach)
        })
    }

    /// Like [`Self::melee_pose_contact`] for a caller that already holds the
    /// weapon node's world transform, such as a pose sampled off the tree.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn melee_bone_contact(&mut self, name: GString, weapon_bone: Transform3D) -> bool {
        self.contact_event(&name.to_string(), |game, attacker, hand| {
            let reach = game.world_mut().melee_reach(attacker, hand)?;
            crate::godot_combat::pose_segment(weapon_bone, reach)
        })
    }

    /// Rebuild actor bodies from the shared World now. Returns how many
    /// actors hold a body, or -1 without a game or physics space.
    #[func]
    pub fn sync_contact_actors(&mut self) -> i64 {
        if self.sync_actor_bodies() {
            self.actor_bodies.len() as i64
        } else {
            -1
        }
    }

    /// The server body presenting one actor, or an invalid RID. Diagnostic
    /// callers may inspect it but must not free it.
    #[func]
    pub fn contact_actor_body(&self, file: i32, object: i64) -> Rid {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return Rid::Invalid;
        };
        self.actor_bodies
            .body(GlobalFormId { file, object })
            .unwrap_or(Rid::Invalid)
    }
}

impl PlayerNode {
    /// Follow the shared World: one body per resident actor, moved to its
    /// current placement, and none for actors that left or were disabled.
    pub(super) fn sync_actor_bodies(&mut self) -> bool {
        let Some(world) = self.base().get_world_3d() else {
            return false;
        };
        let space = world.get_space();
        if !space.is_valid() {
            return false;
        }
        let Some(game) = &self.game else {
            return false;
        };
        let actors = game.world_mut().contact_actors();
        self.actor_bodies.sync(space, &actors);
        true
    }

    /// Route one graph event through the shared contact latch. `segment`
    /// runs only for a HitFrame on an armed swing, after bodies follow the
    /// World, and receives the armed attacker and hand.
    fn contact_event(
        &mut self,
        name: &str,
        segment: impl FnOnce(
            &soe_game::Game,
            GlobalFormId,
            Hand,
        ) -> Result<(Vector3, Vector3), soe_game::combat::Error>,
    ) -> bool {
        let Some(world) = self.base().get_world_3d() else {
            return false;
        };
        let space = world.get_space();
        if name == "HitFrame" && self.melee_contact.armed().is_some() {
            self.sync_actor_bodies();
        }
        let armed = self.melee_contact.armed();
        let Some(game) = &mut self.game else {
            return false;
        };
        // Game::melee_hit needs the Game mutably inside the latch, so the
        // segment is read first and its error is reported through the query.
        let segment = match armed {
            Some((attacker, hand)) if name == "HitFrame" => Some(segment(game, attacker, hand)),
            _ => None,
        };
        let owners = [&self.melee_bodies, self.actor_bodies.owners()];
        let result = self.melee_contact.event(game, name, |attacker| {
            let Some(segment) = segment else {
                return Ok(None);
            };
            let (from, to) = segment?;
            crate::godot_combat::first_contact(space, &owners, attacker, from, to)
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
