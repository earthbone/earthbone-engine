// SPDX-License-Identifier: GPL-3.0-or-later

//! The player adapter writes controller poses into Game before dispatching input.
//! Scripted moves and door landings then flow back into collision and rendering.

use super::{
    godot_error, godot_print, CameraMotion, CharacterSettings, GlobalFormId, PhysicsWorld,
    PlayerNode, Vec3,
};
use godot::obj::WithBaseField;
use godot::prelude::{GString, Variant};

fn restored_player_destination(
    cell: Option<GlobalFormId>,
    position: [f32; 3],
    rotation: [f32; 3],
) -> Option<(GlobalFormId, [f32; 3], [f32; 3])> {
    let cell = cell?;
    (position.into_iter().chain(rotation).all(f32::is_finite)).then_some((cell, position, rotation))
}

impl PlayerNode {
    /// Attach a Game whose vanilla-save restoration has already succeeded.
    /// Installs the restored location and camera, then advances one host frame.
    /// The caller must replace its active Game only after this returns true.
    pub fn attach_restored_game(&mut self, mut game: soe_game::Game) -> bool {
        if self.data_source.is_none()
            || self
                .game_streamer
                .as_ref()
                .is_none_or(|streamer| !streamer.is_instance_valid())
        {
            return false;
        }
        let player = game.player();
        let destination = {
            let world = game.world();
            world.reference_ref(player).and_then(|reference| {
                restored_player_destination(reference.cell, reference.position, reference.rotation)
            })
        };
        if destination.is_none() {
            return false;
        }

        self.game = Some(game);
        self.melee_contact.cancel();
        self.melee_bodies.clear();
        self.last_activate_reference = None;
        self.footstep_cadence.reset();
        if !self.sync_game_pose() {
            self.game = None;
            return false;
        }

        self.advance(1.0 / 60.0);
        self.game.is_some()
            && self.character.is_some()
            && self.loaded_cell.is_some()
            && self
                .camera
                .as_ref()
                .is_some_and(|camera| camera.is_instance_valid())
    }

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
        self.main_menu.leave();
        self.sync_game_pose()
    }

    pub(super) fn advance_game(&mut self, delta: f64) {
        if self.game.is_none() || self.main_menu.is_active() {
            return;
        }
        self.sync_camera_controls();
        let feet = self.feet();
        let rotation = [-self.rig.pitch(), 0.0, self.rig.yaw()];
        if let Some(game) = &mut self.game {
            let player = game.player();
            let prior = game
                .world()
                .reference_ref(player)
                .map(|reference| (reference.cell, reference.position));
            let cell = {
                let mut world = game.world_mut();
                // The previous frame has already installed the new host resources.
                world.settle_unload();
                if let Some(reference) = world.reference(player) {
                    reference.position = [feet.x, feet.y, feet.z];
                    reference.rotation = rotation;
                }
                let should_stand = world
                    .furniture_seat(player)
                    .and_then(|(furniture, _)| {
                        let (actor_position, actor_cell) = world
                            .reference_ref(player)
                            .map(|actor| (actor.position, actor.cell))?;
                        let seat = world.reference(furniture)?;
                        let moved = actor_position
                            .iter()
                            .zip(seat.position)
                            .map(|(actor, seat)| (*actor - seat).powi(2))
                            .sum::<f32>()
                            > 24.0_f32.powi(2);
                        Some(moved || actor_cell != seat.cell)
                    })
                    .unwrap_or(false);
                if should_stand {
                    world.stand_from_furniture(player);
                }
                world
                    .reference_ref(player)
                    .and_then(|reference| reference.cell)
            };
            let from = prior.map_or([feet.x, feet.y, feet.z], |(_, position)| position);
            game.update_trigger_actor_position(player, cell, from, [feet.x, feet.y, feet.z]);
        }
        // CONTEXT: Apply game edits before reading simulated body poses back
        // into World, then draw the shared placement on this tick.
        if let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) {
            let mut world = game.world_mut();
            streamer.bind_mut().sync_reference_changes(&mut world);
        }
        if !self.advance_exterior() {
            self.auto_step = false;
            return;
        }
        if let Some(target) = self.last_activate_reference {
            self.activate_game_reference(target);
        }
        if let Some(game) = &mut self.game {
            self.menu.tick(game, delta);
            self.image_space.sync(&game.world());
            let (hud_cart_mode, requests) = {
                let mut world = game.world_mut();
                (world.hud_cart_mode, world.take_game_ui_requests())
            };
            if hud_cart_mode != self.presented_hud_cart_mode {
                self.presented_hud_cart_mode = hud_cart_mode;
                self.base_mut()
                    .emit_signal("hud_cart_mode_changed", &[Variant::from(hud_cart_mode)]);
            }
            for request in requests {
                match request {
                    soe_game::world::GameUiRequest::ShowRaceMenu => {
                        self.base_mut().emit_signal("race_menu_requested", &[]);
                    }
                    soe_game::world::GameUiRequest::TriggerScreenBlood(value) => {
                        self.base_mut()
                            .emit_signal("screen_blood_requested", &[Variant::from(value)]);
                    }
                    soe_game::world::GameUiRequest::PlayBink { file_name, options } => {
                        self.base_mut().emit_signal(
                            "bink_requested",
                            &[
                                Variant::from(GString::from(file_name.as_str())),
                                Variant::from(options.interruptible),
                                Variant::from(options.mute_audio),
                                Variant::from(options.mute_music),
                                Variant::from(options.letterbox),
                            ],
                        );
                    }
                }
            }
        }
        self.capture_screenshot_requests();
        let training_request = self
            .game
            .as_mut()
            .and_then(|game| game.world_mut().take_training_menu_request());
        if let Some(trainer) = training_request {
            self.open_training_menu(trainer);
        }
        let requests = self.game.as_mut().map_or_else(Vec::new, |game| {
            std::mem::take(&mut game.world_mut().menu_requests)
        });
        for request in requests {
            match request {
                soe_game::world::MenuRequest::PrecacheCharGen => {
                    if let (Some(game), Some(streamer)) =
                        (self.game.as_ref(), self.game_streamer.as_mut())
                    {
                        streamer.bind_mut().precache_chargen(game);
                    }
                }
                soe_game::world::MenuRequest::PrecacheCharGenClear => {
                    if let Some(streamer) = self.game_streamer.as_mut() {
                        streamer.bind_mut().precache_chargen_clear();
                    }
                }
                soe_game::world::MenuRequest::Race => {
                    self.show_race_menu();
                }
            }
        }
        if let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) {
            let requests = game.world_mut().take_effect_shader_requests();
            let world = game.world();
            streamer
                .bind_mut()
                .sync_effect_shaders(&world, requests, delta.max(0.0) as f32);
        }
        self.apply_host_requests();
        if self.main_menu.is_active() {
            return;
        }
        if self.sync_game_pose() {
            self.prefetch_nearby_interior(delta);
        } else {
            self.auto_step = false;
            godot_error!("PlayerNode: gameplay stopped because destination resources failed");
        }
    }

    pub(super) fn sync_camera_controls(&mut self) {
        let Some(game) = &self.game else {
            return;
        };
        let controls = game.world().camera_controls;
        self.camera_state.apply(controls);
        self.rig
            .set_camera_yaw_offset_degrees(self.camera_state.sitting_rotation_degrees());
        if let Some(visual) = &mut self.player_appearance {
            if visual.is_instance_valid() {
                visual.bind_mut().set_first_person_geometry_visible(
                    self.rig.mode != super::CameraMode::First
                        || self.camera_state.first_person_geometry_visible(),
                );
            } else {
                self.player_appearance = None;
            }
        }
    }

    pub(super) fn capture_screenshot_requests(&mut self) -> bool {
        let requests = self
            .game
            .as_mut()
            .map(|game| game.world_mut().take_screenshot_requests())
            .unwrap_or_default();
        let mut succeeded = true;
        for path in requests {
            let Some(viewport) = self.base().get_viewport() else {
                godot_error!("PlayerNode: screenshot has no viewport for {path}");
                succeeded = false;
                continue;
            };
            let Some(texture) = viewport.get_texture() else {
                godot_error!("PlayerNode: screenshot viewport has no texture for {path}");
                succeeded = false;
                continue;
            };
            let Some(image) = texture.get_image() else {
                godot_error!("PlayerNode: screenshot viewport has no image for {path}");
                succeeded = false;
                continue;
            };
            if image.save_png(&path) != godot::global::Error::OK {
                godot_error!("PlayerNode: failed to save screenshot to {path}");
                succeeded = false;
            }
        }
        succeeded
    }

    fn apply_host_requests(&mut self) {
        let requests = self.game.as_mut().map_or_else(Vec::new, |game| {
            std::mem::take(&mut game.world_mut().host_requests)
        });
        for request in requests {
            match request {
                soe_game::world::HostRequest::QuitToMainMenu => {
                    self.enter_main_menu();
                }
                soe_game::world::HostRequest::StartTitleSequence(name) => {
                    self.enter_main_menu();
                    self.main_menu.start_sequence(name.clone());
                    self.base_mut().emit_signal(
                        "title_sequence_started",
                        &[godot::builtin::Variant::from(GString::from(name.as_str()))],
                    );
                }
            }
        }
    }

    fn enter_main_menu(&mut self) {
        if self.main_menu.is_active() {
            return;
        }
        self.close_menu();
        if let Some(streamer) = self.game_streamer.as_mut() {
            streamer.bind_mut().clear();
        }
        self.game = None;
        self.data_source = None;
        self.game_streamer = None;
        self.loaded_cell = None;
        self.exterior = None;
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        self.collision_references.clear();
        self.melee_bodies.clear();
        self.last_melee_target = None;
        self.last_melee_obstruction = None;
        self.world = None;
        self.query_collision.free();
        self.actor_bodies.free();
        self.character = None;
        self.main_menu.enter();
        self.base_mut().emit_signal("returned_to_main_menu", &[]);
    }

    fn prefetch_nearby_interior(&mut self, delta: f64) {
        use crate::interior_prefetch::{APPROACH_RADIUS, SELECTION_INTERVAL};
        self.interior_prefetch.poll();
        self.interior_prefetch_elapsed += delta.max(0.0);
        if self.interior_prefetch_elapsed < SELECTION_INTERVAL {
            return;
        }
        self.interior_prefetch_elapsed = 0.0;
        let (Some(game), Some(cell), Some(source)) =
            (&self.game, self.loaded_cell, &self.data_source)
        else {
            return;
        };
        let feet = self.feet();
        let destination = game.world().nearby_interior_destination(
            cell,
            [feet.x, feet.y, feet.z],
            APPROACH_RADIUS,
        );
        if let Some(destination) = destination {
            self.interior_prefetch
                .request(std::sync::Arc::clone(source), destination.cell);
        } else {
            self.interior_prefetch.invalidate();
        }
    }

    pub(super) fn activate_game_reference(&mut self, target: GlobalFormId) -> bool {
        let (activated, show_container, show_crafting, lockpick_request) = {
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
            if game.world_mut().can_sleep_at_furniture(target) {
                let activated = game.activate_deferred(target, player);
                return activated && game.sleep_player_at_furniture(target, 8.0, false);
            }
            let (container, blocked) = {
                let mut world = game.world_mut();
                let container = world.base_of(target).is_some_and(|base| {
                    world
                        .tag_of(base)
                        .is_some_and(|tag| tag.as_bytes() == b"CONT")
                });
                let blocked = world
                    .reference_ref(target)
                    .is_some_and(|reference| reference.activation_blocked);
                (container, blocked)
            };
            let creation_bench = crate::crafting_menu::is_creation_bench(game, target);
            let lockpick = game.world().form("Lockpick");
            let key = game.world().key(target);
            let has_key = key.is_some_and(|key| game.world_mut().item_count(player, key) > 0);
            let lockpick_request = !blocked
                && game.world().reference_ref(target).is_some_and(|reference| {
                    reference.locked && reference.lock_level != 255 && !has_key
                })
                && lockpick.is_some_and(|pick| game.world_mut().item_count(player, pick) > 0);
            let activated = if lockpick_request {
                false
            } else {
                game.activate_deferred(target, player)
            };
            let opened = game
                .world_mut()
                .reference_ref(target)
                .is_some_and(|reference| reference.opened);
            (
                activated,
                activated && container && !blocked && opened,
                activated && creation_bench && !blocked,
                lockpick_request,
            )
        };
        if lockpick_request {
            self.pending_lockpick = Some(target);
            if self.open_lockpicking_menu(target) {
                self.pending_lockpick = None;
            }
            return false;
        }
        self.pending_lockpick = None;
        if show_container {
            self.open_container_menu(target);
        } else if show_crafting {
            self.open_crafting_menu(i64::from(target.file), i64::from(target.object));
        }
        activated
    }

    pub(super) fn sync_game_pose(&mut self) -> bool {
        let destination = {
            let Some(game) = &mut self.game else {
                return true;
            };
            let player = game.player();
            game.world()
                .reference_ref(player)
                .and_then(|r| r.cell.map(|cell| (cell, r.position, r.rotation)))
        };
        let Some((cell, position, rotation)) = destination else {
            godot_error!("PlayerNode: game player has no destination cell");
            return false;
        };
        let feet = Vec3::new(position[0], position[1], position[2]);
        let needs_exterior = self.exterior.is_none()
            && self
                .data_source
                .as_ref()
                .and_then(|source| source.exterior_world(cell))
                .is_some();
        if self.loaded_cell != Some(cell) || needs_exterior {
            // Contacts belong to the cell that is leaving, not to an exterior
            // ring that is only being attached.
            if self.loaded_cell != Some(cell) {
                self.melee_contact.cancel();
                self.melee_bodies.clear();
            }
            let Some(source) = self.data_source.as_ref() else {
                return false;
            };
            let worldspace = source.exterior_world(cell);
            let prepared = self.interior_prefetch.take(cell);
            self.interior_prefetch.invalidate();
            let location =
                match prepared.map_or_else(|| source.load_location(cell, position, 1), Ok) {
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
            // Door transitions already attach the destination and defer its old focus.
            // Other cells in an old exterior ring leave with their host resources.
            if let Some(previous) = self.exterior.take() {
                let Some(game) = self.game.as_mut() else {
                    return false;
                };
                let mut runtime = game.world_mut();
                let pending = runtime.pending_unload();
                let departing: Vec<_> = previous
                    .cells
                    .keys()
                    .copied()
                    .filter(|id| Some(*id) != pending && *id != cell)
                    .collect();
                if !departing.is_empty() {
                    if let Err(error) = runtime.detach_streaming_cells(&departing) {
                        godot_error!("PlayerNode: {error}");
                        return false;
                    }
                }
            }
            if let Some(worldspace) = worldspace {
                let mut planner = crate::streaming::CellStreamer::new(super::exterior::config())
                    .map_err(|error| godot_error!("PlayerNode: {error}"));
                let Ok(ref mut planner) = planner else {
                    return false;
                };
                planner.settle(crate::units::SkyrimPos::new(
                    position[0],
                    position[1],
                    position[2],
                ));
                let entered: Vec<_> = location
                    .cells
                    .iter()
                    .map(|(id, _)| *id)
                    .filter(|id| *id != cell)
                    .collect();
                let Some(game) = self.game.as_mut() else {
                    return false;
                };
                if let Err(error) = game.world_mut().attach_streaming_cells(&entered) {
                    godot_error!("PlayerNode: {error}");
                    return false;
                }
                self.exterior = Some(super::exterior::ExteriorRuntime {
                    world: worldspace,
                    planner: planner.clone(),
                    cells: location.collision_cells,
                });
            }
            let collision = location.collision;
            let mut world = collision.world;
            let id = world.create_character(CharacterSettings::default(), feet);
            self.world = Some(world);
            self.character = Some(id);
            self.collision_references = collision.references;
            self.loaded_cell = Some(cell);
            self.sync_cell_ambience(cell);
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

#[cfg(test)]
mod sav_19_tests {
    use super::restored_player_destination;
    use soe_esm::GlobalFormId;

    #[test]
    fn sav_19_restored_player_destination_requires_cell_and_finite_pose() {
        let cell = GlobalFormId {
            file: 0,
            object: 0x1234,
        };
        let pose = restored_player_destination(Some(cell), [1.0, 2.0, 3.0], [0.0; 3]);
        assert_eq!(pose, Some((cell, [1.0, 2.0, 3.0], [0.0; 3])));
        assert!(restored_player_destination(None, [1.0, 2.0, 3.0], [0.0; 3]).is_none());
        assert!(restored_player_destination(Some(cell), [f32::NAN, 2.0, 3.0], [0.0; 3]).is_none());
    }
}
