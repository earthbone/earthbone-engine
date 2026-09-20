// SPDX-License-Identifier: GPL-3.0-or-later

//! The player adapter writes controller poses into Game before dispatching input.
//! Scripted moves and door landings then flow back into collision and rendering.

use super::{
    godot_error, godot_print, CameraMotion, CharacterSettings, GlobalFormId, PhysicsWorld,
    PlayerNode, Vec3,
};

impl PlayerNode {
    pub(super) fn start_game_runtime(&mut self) -> bool {
        if self.game.is_some() {
            return true;
        }
        let (Some(source), Some(cell), Some(_)) = (
            self.data_source.as_ref(),
            self.loaded_cell,
            self.game_streamer.as_ref(),
        ) else {
            godot_error!(
                "PlayerNode: share streamer data and enter a cell before starting gameplay"
            );
            return false;
        };
        let mut game = soe_game::Game::from_shared_order(source.shared_order());
        let scripts = game.load_scripts_from_vfs(source.vfs());
        if !scripts.scripts_failed.is_empty() {
            godot_error!(
                "PlayerNode: failed to load {} scripts",
                scripts.scripts_failed.len()
            );
            return false;
        }
        let player = game.player();
        let feet = self.feet();
        let report = game.start();
        godot_print!(
            "GAMEPLAY scripts={} attached={} missing={} quests={}",
            scripts.scripts_loaded,
            report.scripts_attached,
            report.scripts_missing.len(),
            report.quests_started
        );
        // This entry point is a play-from-here walkthrough, not the new-game intro.
        // Startup quests may move the player, so apply the requested spawn afterwards.
        if let Some(reference) = game.world_mut().reference(player) {
            reference.cell = Some(cell);
            reference.position = [feet.x, feet.y, feet.z];
            reference.rotation = [-self.rig.pitch(), 0.0, self.rig.yaw()];
        }
        game.world_mut().change_cell(Some(cell));
        game.run_until_idle(64);
        self.image_space.sync(&game.world());
        self.start_footstep_audio();
        self.game = Some(game);
        self.sync_game_pose()
    }

    pub(super) fn advance_game(&mut self, delta: f64) {
        if self.game.is_none() {
            return;
        }
        let feet = self.feet();
        let rotation = [-self.rig.pitch(), 0.0, self.rig.yaw()];
        if let Some(game) = &mut self.game {
            let player = game.player();
            {
                let mut world = game.world_mut();
                // The previous frame has already installed the new host resources.
                world.settle_unload();
                if let Some(reference) = world.reference(player) {
                    reference.position = [feet.x, feet.y, feet.z];
                    reference.rotation = rotation;
                }
            }
        }
        if let Some(target) = self.last_activate_reference {
            self.activate_game_reference(target);
        }
        if let Some(game) = &mut self.game {
            self.menu.tick(game, delta);
            self.image_space.sync(&game.world());
        }
        if !self.sync_game_pose() {
            self.auto_step = false;
            godot_error!("PlayerNode: gameplay stopped because destination resources failed");
        }
    }

    pub(super) fn activate_game_reference(&mut self, target: GlobalFormId) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        // A player pick must not interact with an invisible/deleted reference.
        // The shared game path handles locks and inventory keys.
        let eligible = game
            .world_mut()
            .reference(target)
            .is_some_and(|r| r.enabled && !r.deleted);
        if !eligible {
            return false;
        }
        let player = game.player();
        game.activate_deferred(target, player)
    }

    pub(super) fn sync_game_pose(&mut self) -> bool {
        let Some(game) = &mut self.game else {
            return true;
        };
        let player = game.player();
        let Some((cell, position, rotation)) = game
            .world()
            .reference_ref(player)
            .and_then(|r| r.cell.map(|cell| (cell, r.position, r.rotation)))
        else {
            godot_error!("PlayerNode: game player has no destination cell");
            return false;
        };
        let feet = Vec3::new(position[0], position[1], position[2]);
        if self.loaded_cell != Some(cell) {
            self.melee_contact.cancel();
            self.melee_bodies.clear();
            let Some(source) = self.data_source.as_ref() else {
                return false;
            };
            let location = match source.load_location(cell, position, 1) {
                Ok(location) => location,
                Err(error) => {
                    godot_error!("PlayerNode: {error}");
                    return false;
                }
            };
            let Some(mut query_collision) = self.prepare_query_collision(&location.collision)
            else {
                return false;
            };

            let texture_preparation = location
                .cells
                .iter()
                .map(|(_, loaded)| loaded.stats.texture_decode_time)
                .sum();
            let Some(streamer) = self.game_streamer.as_mut() else {
                query_collision.free();
                return false;
            };
            if !streamer.is_instance_valid() || !streamer.bind_mut().replace_location(&location) {
                query_collision.free();
                return false;
            }
            self.query_collision.free();
            self.query_collision = query_collision;
            let collision = location.collision;
            let mut world = collision.world;
            let id = world.create_character(CharacterSettings::default(), feet);
            self.world = Some(world);
            self.character = Some(id);
            self.collision_references = collision.references;
            self.loaded_cell = Some(cell);
            self.record_door_texture_preparation(texture_preparation);
            self.footstep_cadence.reset();
            self.motion = CameraMotion::new(feet, self.capsule_height());
            godot_print!(
                "GAMEPLAY cell={cell} position={position:?} bodies={} texture_prepare_ms={:.3} fade={}",
                collision.stats.bodies,
                texture_preparation.as_secs_f64() * 1000.0,
                self.door_fade_active()
            );
        } else if self.feet() != feet {
            self.footstep_cadence.reset();
            if let Some(world) = &mut self.world {
                if let Some(old) = self.character.take() {
                    world.remove(old);
                }
                self.character = Some(world.create_character(CharacterSettings::default(), feet));
            }
            self.motion = CameraMotion::new(feet, self.capsule_height());
        }
        self.rig.set_facing(rotation[2], -rotation[0]);
        self.update_camera();
        true
    }
}
