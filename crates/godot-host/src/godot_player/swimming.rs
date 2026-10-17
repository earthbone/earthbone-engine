// SPDX-License-Identifier: GPL-3.0-or-later

//! Record water and race movement drive the existing capsule and actor graphs.

use super::{InputState, PlayerNode};
use godot::prelude::godot_api;
use soe_physics::{PhysicsWorld, Vec3};

impl PlayerNode {
    pub(super) fn advance_swimming(&mut self, input: &InputState, delta: f32) -> bool {
        let feet = self.feet();
        let height = self
            .player_appearance
            .as_ref()
            .filter(|visual| visual.is_instance_valid())
            .and_then(|visual| visual.bind().camera_head_height())
            .unwrap_or_else(|| self.capsule_height());
        let Some(game) = &mut self.game else {
            return false;
        };
        let player = game.player();
        let (cell, was_swimming, drawn, fraction) = {
            let world = game.world();
            (
                world
                    .reference_ref(player)
                    .and_then(|reference| reference.cell)
                    .or(self.loaded_cell),
                world.actor_ref(player).is_some_and(|actor| actor.swimming),
                world
                    .actor_ref(player)
                    .is_some_and(|actor| actor.weapon_drawn),
                world
                    .game_setting("fSwimHeightScale")
                    .map(|value| value.as_float())
                    .unwrap_or(0.65),
            )
        };
        let water = cell
            .and_then(|cell| {
                self.data_source
                    .as_ref()?
                    .water_at(cell, [feet.x, feet.y, feet.z])
            })
            .map(|(height, _)| height);
        let swimming = crate::swimming::submerged(was_swimming, feet.z, water, height, fraction);
        if let Some(actor) = game.world_mut().actor(player) {
            actor.swimming = swimming;
            if swimming {
                actor.sneaking = false;
                actor.sprinting = false;
            }
        }
        if !swimming {
            return false;
        }
        let rates = game.world_mut().actor_swim_movement(player);
        let paralyzed = game.world().actor_is_paralyzed(player);
        if drawn {
            self.toggle_weapon_ready();
        }
        self.driver.sneaking = false;
        let rates = if input.walk { rates.walk } else { rates.run };
        let speed =
            rates
                .speed(input.move_x, input.move_y)
                .max(if input.jump_held || input.sneak_held {
                    rates.forward
                } else {
                    0.0
                });
        let forward = self.rig.look_direction();
        let right = Vec3::new(self.rig.yaw().cos(), -self.rig.yaw().sin(), 0.0);
        let mut desired = forward * input.move_y + right * input.move_x;
        desired.z += f32::from(input.jump_held) - f32::from(input.sneak_held);
        if desired.length() > 1.0 {
            desired = desired.normalized();
        }
        desired = desired * speed;
        if paralyzed {
            desired = Vec3::ZERO;
        }
        let head = was_swimming
            .then(|| {
                self.player_appearance
                    .as_ref()
                    .and_then(|visual| visual.bind().posed_head_height())
            })
            .flatten();
        let depth = crate::swimming::surface_depth(height, head, fraction);
        if let (Some(world), Some(id), Some(water)) = (&mut self.world, self.character, water) {
            world.set_crouch(id, false);
            world.step_swimming(id, desired, water, depth, delta);
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Shared swimming state used by conditions and Papyrus queries.
    #[func]
    pub fn is_swimming(&self) -> bool {
        self.game.as_ref().is_some_and(|game| {
            let mut world = game.world_mut();
            let player = world.player();
            world.actor_ref(player).is_some_and(|actor| actor.swimming)
        })
    }
}
