// SPDX-License-Identifier: GPL-3.0-or-later

//! The player adapter writes controller poses into Game before dispatching input.
//! Scripted moves and door landings then flow back into collision and rendering.

use super::{CameraMotion, CharacterSettings, GlobalFormId, PhysicsWorld, PlayerNode, Vec3};
use crate::units::placement_to_godot;
use godot::prelude::*;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};
use std::collections::{BTreeMap, BTreeSet};

fn should_stand_from_furniture(world: &mut soe_game::World, player: GlobalFormId) -> bool {
    world
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
        .unwrap_or(false)
}

fn restored_player_destination(
    cell: Option<GlobalFormId>,
    position: [f32; 3],
    rotation: [f32; 3],
) -> Option<(GlobalFormId, [f32; 3], [f32; 3])> {
    let cell = cell?;
    (position.into_iter().chain(rotation).all(f32::is_finite)).then_some((cell, position, rotation))
}

impl PlayerNode {
    /// Invoke either main-menu Game native for host integration checks.
    pub(super) fn invoke_main_menu_native_for_check(
        &mut self,
        name: &GString,
        sequence: &GString,
    ) -> bool {
        let name = name.to_string();
        let native_args = match name.as_str() {
            "QuitToMainMenu" => Vec::new(),
            "StartTitleSequence" => vec![ident("sequence")],
            _ => return false,
        };
        let script_name = "SoeUi18MainMenuHostCheck";
        let mut function = FunctionBuilder::new("Run", "None")
            .global()
            .local("result", "None");
        let mut operands = vec![s("Game"), s(&name), ident("result")];
        if name == "StartTitleSequence" {
            function = function.param("sequence", "String");
        }
        operands.extend(native_args);
        let bytes = ScriptBuilder::new(script_name, "Form")
            .function(function.ins(CallStatic, operands).build())
            .bytes();
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        if let Err(error) = game.load_script(&bytes) {
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: failed to load UI-18 host check script: {error}");
            return false;
        }
        let args = if name == "StartTitleSequence" {
            vec![Value::string(&sequence.to_string())]
        } else {
            Vec::new()
        };
        let thread = match game.vm.call_global(script_name, "Run", &args) {
            Ok(thread) => thread,
            Err(error) => {
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: Game.{name} host check failed: {error}");
                return false;
            }
        };
        if !game.run_until_idle(64) || game.vm.thread_result(thread) != Some(Value::None) {
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: Game.{name} host check did not complete");
            return false;
        }
        self.apply_host_requests();
        true
    }

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
        let Some((cell, position, rotation)) = destination else {
            return false;
        };

        let previous_detection = std::mem::take(&mut self.automatic_detection);
        let previous_game = self.game.replace(game);
        self.clear_player_input();
        self.melee_contact.cancel();
        self.melee_bodies.clear();
        self.last_activate_reference = None;
        self.footstep_cadence.reset();
        if !self.sync_game_pose() {
            self.game = previous_game;
            self.automatic_detection = previous_detection;
            return false;
        }

        self.advance(1.0 / 60.0);
        if let Some(game) = &self.game {
            if let Some(reference) = game.world_mut().reference(player) {
                reference.cell = Some(cell);
                reference.position = position;
                reference.rotation = rotation;
            }
        }
        let pose_reinstalled = self.sync_game_pose();
        if let Some(camera) = &mut self.camera {
            camera.make_current();
        }
        let attached = pose_reinstalled
            && self.game.is_some()
            && self.character.is_some()
            && self.loaded_cell.is_some()
            && self
                .camera
                .as_ref()
                .is_some_and(|camera| camera.is_instance_valid());
        if !attached {
            self.game = previous_game;
            self.automatic_detection = previous_detection;
        }
        attached
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
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay",
                "PlayerNode: share streamer data and enter a cell before starting gameplay"
            );
            return false;
        };
        let mut game = match source.localized_game() {
            Ok(game) => game,
            Err(error) => {
                godot_error!("PlayerNode: {error}");
                return false;
            }
        };
        if let Some(directory) = source.profile_papyrus_logs().1 {
            if let Err(error) =
                game.configure_papyrus_logs(source.profile_papyrus_logs().0, directory)
            {
                soe_trace::warn!(target: "host::gameplay", system = "host::gameplay",
                    error = %error, "PlayerNode: could not configure Papyrus file logs");
            }
        }
        let scripts = game.load_scripts_from_vfs(source.vfs());
        if !scripts.scripts_failed.is_empty() {
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay",
                "PlayerNode: failed to load {} scripts",
                scripts.scripts_failed.len()
            );
            return false;
        }
        let player = game.player();
        let feet = self.feet();
        let report = game.start();
        soe_trace::info!(target: "host::gameplay", system = "host::gameplay",
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
        game.change_cell(Some(cell));
        game.run_until_idle(64);
        if let Err(error) = self.image_space.sync(&game.world()) {
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
        }
        self.effect_shaders.sync(&game.world());
        self.automatic_detection = super::detection::AutomaticDetection::default();
        self.game = Some(game);
        self.cap_catchup_steps();
        self.start_footstep_audio();
        self.clear_player_input();
        self.refresh_image_space();
        self.main_menu.leave();
        let installed = self.sync_game_pose();
        self.sync_reference_collision();
        installed
    }

    /// Most physics ticks Godot may run for one presentation frame while the
    /// legacy inline game is live. Godot's default of 8 makes a slow frame run
    /// the whole game up to eight times, which makes the next frame slower
    /// again. Two keeps one tick of catch-up and lets game time fall behind wall
    /// time instead of spiralling (PRF-37 clause 1, interim until the game
    /// leaves the render thread).
    const CATCHUP_STEP_CAP: i32 = 2;

    fn cap_catchup_steps(&mut self) {
        let mut engine = godot::classes::Engine::singleton();
        let current = engine.get_max_physics_steps_per_frame();
        if self.prior_max_physics_steps.is_none() && current > Self::CATCHUP_STEP_CAP {
            self.prior_max_physics_steps = Some(current);
            engine.set_max_physics_steps_per_frame(Self::CATCHUP_STEP_CAP);
        }
    }

    pub(super) fn restore_catchup_cap(&mut self) {
        if let Some(prior) = self.prior_max_physics_steps.take() {
            godot::classes::Engine::singleton().set_max_physics_steps_per_frame(prior);
        }
    }

    /// Runs both reference syncs, collision and scene changes, once.
    fn run_reference_sync(&mut self) {
        if let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) {
            let mut world = game.world_mut();
            streamer.bind_mut().sync_reference_changes(&mut world);
        }
        self.sync_reference_collision();
    }

    /// Automatic physics ticks only mark the sync pending, because the scene
    /// draws once per presentation frame and each catch-up tick repeated the same
    /// cost. `process` runs it once, after the last tick, so one frame always
    /// pays for one pass. Manual `advance` calls sync at once, which keeps
    /// scripted fixed-step runs unchanged.
    fn sync_references_for_tick(&mut self) {
        if self.automatic_physics_step {
            self.reference_sync_pending = true;
            return;
        }
        self.run_reference_sync();
    }

    /// Applies the sync that automatic ticks deferred, once per presentation frame.
    pub(super) fn flush_reference_sync(&mut self) {
        if std::mem::take(&mut self.reference_sync_pending) {
            self.run_reference_sync();
        }
    }

    pub(super) fn advance_game(&mut self, delta: f64, sprinting: bool) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Render);
        let _scene = crate::scene_timing::Scope::new(soe_trace::System::Other);
        if self.game.is_none() || self.main_menu.is_active() {
            return;
        }
        if let Some(game) = &self.game {
            self.query_collision
                .set_collisions_enabled(game.world().debug_collisions_enabled);
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
                if let Some(actor) = world.actor(player) {
                    actor.sprinting = sprinting;
                }
                if let Some(reference) = world.reference(player) {
                    reference.position = [feet.x, feet.y, feet.z];
                    reference.rotation = rotation;
                }
                if should_stand_from_furniture(&mut world, player) {
                    world.stand_from_furniture(player);
                }
                let look = self.rig.look_direction();
                world.camera_look = [look.x, look.y, look.z];
                world
                    .reference_ref(player)
                    .and_then(|reference| reference.cell)
            };
            let from = prior.map_or([feet.x, feet.y, feet.z], |(_, position)| position);
            game.update_trigger_actor_position(player, cell, from, [feet.x, feet.y, feet.z]);
            if delta.is_finite() && delta > 0.0 {
                game.tick(delta);
            }
        }
        // CONTEXT: Apply game edits before reading simulated body poses back
        // into World, then draw the shared placement on this tick.
        self.sync_references_for_tick();
        if !self.advance_exterior() {
            self.auto_step = false;
            return;
        }
        if let Some(target) = self.last_activate_reference {
            self.activate_game_reference(target);
        }
        self.process_debug_host_requests();
        if let Some(game) = &mut self.game {
            if let Err(error) = self.image_space.sync(&game.world()) {
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
            }
            game.deliver_animation_reports();
            let shakes = std::mem::take(&mut game.world_mut().camera_shakes);
            for shake in shakes {
                self.camera_shake = Some((0.0, shake.duration, shake.strength));
                self.camera_shake_time = 0.0;
            }
            let hud_cart_mode = game.world().hud_cart_mode;
            if hud_cart_mode != self.presented_hud_cart_mode {
                self.presented_hud_cart_mode = hud_cart_mode;
                self.base_mut()
                    .emit_signal("hud_cart_mode_changed", &[Variant::from(hud_cart_mode)]);
            }
        }
        if !self.auto_step {
            self.refresh_image_space();
        }

        self.advance_ambient_dialogue();
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
                // CONTEXT: Valid trainers use the pending request consumed above.
                soe_game::world::MenuRequest::Training(_) => {}
                soe_game::world::MenuRequest::Barter(actor) => {
                    self.open_barter_menu(actor);
                }
                soe_game::world::MenuRequest::ActorInventory(actor) => {
                    self.open_container_menu(actor);
                }
            }
        }
        if let Some(game) = &mut self.game {
            let requests = game.world_mut().take_game_ui_requests();
            for request in requests {
                match request {
                    soe_game::world::GameUiRequest::ShowRaceMenu => {
                        self.base_mut().emit_signal("race_menu_requested", &[]);
                    }
                    soe_game::world::GameUiRequest::TriggerScreenBlood(value) => {
                        if let Some(view) = self.hud_view.as_mut() {
                            view.trigger_screen_blood(value);
                        }
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
        if let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) {
            let detections = {
                let mut world = game.world_mut();
                match crate::detect_life::presentations(&mut world) {
                    Ok(detections) => detections,
                    Err(error) => {
                        soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: Detect Life presentation failed: {error}");
                        Vec::new()
                    }
                }
            };
            let requests = game.world_mut().take_effect_shader_requests();
            let world = game.world();
            streamer.bind_mut().sync_effect_shaders(
                &world,
                requests,
                &detections,
                delta.max(0.0) as f32,
            );
            streamer.bind_mut().sync_visual_effects(&world);
        }
        if let Some(game) = &self.game {
            self.effect_shaders.sync(&game.world());
        }
        if let (Some(game), Some(streamer)) = (&self.game, self.game_streamer.as_mut()) {
            let world = game.world();
            let layers = world.shader_particle_layers();
            let player = soe_esm::GlobalFormId {
                file: 0,
                object: soe_game::world::PLAYER_OBJECT,
            };
            let origin = world
                .ref_placement(player)
                .filter(|placement| placement.visible)
                .map_or([0.0; 3], |placement| placement.position);
            streamer
                .bind_mut()
                .sync_shader_particles(&layers, origin, delta.max(0.0) as f32);
        }
        self.apply_host_requests();
        if self.main_menu.is_active() {
            return;
        }
        if self.sync_game_pose() {
            self.sync_debug_ref_markers();
            if let Some(game) = &mut self.game {
                for latent in self.pending_debug_latents.drain(..) {
                    game.vm
                        .resolve_latent(latent, soe_papyrus::Value::Float(0.0));
                }
            }
            self.prefetch_nearby_interior(delta);
        } else {
            if let Some(game) = &mut self.game {
                for latent in self.pending_debug_latents.drain(..) {
                    game.vm
                        .resolve_latent(latent, soe_papyrus::Value::Float(-1.0));
                }
            }
            self.auto_step = false;
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: gameplay stopped because destination resources failed");
        }
    }

    pub(super) fn sync_camera_controls(&mut self) {
        let Some(game) = &mut self.game else {
            return;
        };
        game.world_mut()
            .set_player_first_person(self.rig.mode == super::CameraMode::First);
        let controls = game.world().camera_controls;
        self.camera_state.apply(controls);
        self.rig
            .set_camera_yaw_offset_degrees(self.camera_state.sitting_rotation_degrees());
        if let Some(visual) = &mut self.player_appearance {
            if visual.is_instance_valid() {
                visual.bind_mut().set_first_person_geometry_visible(
                    self.camera_state
                        .rendered_first_person_geometry_visible(self.rig.mode),
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
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: screenshot has no viewport for {path}");
                succeeded = false;
                continue;
            };
            let Some(texture) = viewport.get_texture() else {
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: screenshot viewport has no texture for {path}");
                succeeded = false;
                continue;
            };
            let Some(image) = texture.get_image() else {
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: screenshot viewport has no image for {path}");
                succeeded = false;
                continue;
            };
            if image.save_png(&path) != godot::global::Error::OK {
                soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: failed to save screenshot to {path}");
                succeeded = false;
            }
        }
        succeeded
    }

    fn apply_host_requests(&mut self) {
        let requests = self
            .game
            .as_mut()
            .map_or_else(Vec::new, |game| game.world_mut().take_host_requests());
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
                soe_game::HostRequest::CameraFirstPerson => {
                    crate::camera::apply_game_camera_request(&mut self.rig, true);
                    self.update_camera();
                }
                soe_game::HostRequest::CameraThirdPerson => {
                    crate::camera::apply_game_camera_request(&mut self.rig, false);
                    self.update_camera();
                }
                soe_game::HostRequest::ControllerShake {
                    left_strength,
                    right_strength,
                    duration,
                } => {
                    let mut input = godot::classes::Input::singleton();
                    for device in input.get_connected_joypads().iter_shared() {
                        input
                            .start_joy_vibration_ex(device as i32, left_strength, right_strength)
                            .duration(duration)
                            .done();
                    }
                }
                soe_game::HostRequest::FadeOutGame {
                    fading_out,
                    black,
                    seconds_before_fade,
                    fade_duration,
                } => {
                    if !self.game_fade.request(
                        fading_out,
                        black,
                        seconds_before_fade,
                        fade_duration,
                    ) {
                        soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: rejected invalid Game.FadeOutGame timing");
                    }
                    self.refresh_door_fade();
                }
                soe_game::HostRequest::Save { automatic } => {
                    self.write_requested_save(automatic);
                }
            }
        }
    }

    fn process_debug_host_requests(&mut self) {
        let mut scene_tree = self.base().get_tree();
        let Some(game) = &mut self.game else {
            return;
        };
        let requests = std::mem::take(&mut game.world_mut().debug_host_requests);
        for request in requests {
            match request {
                soe_game::world::DebugHostRequest::Quit => scene_tree.quit(),
                soe_game::world::DebugHostRequest::CenterOnCell { cell, latent } => {
                    let player = game.player();
                    if let Some(reference) = game.world_mut().reference(player) {
                        reference.position = [0.0; 3];
                        reference.cell = Some(cell);
                    }
                    game.change_cell(Some(cell));
                    game.run_until_idle(64);
                    if let Some(latent) = latent {
                        self.pending_debug_latents.push(latent);
                    }
                }
                soe_game::world::DebugHostRequest::MovePlayerToReference { reference, latent } => {
                    if game.world_mut().debug_player_move_to_reference(reference) {
                        game.run_until_idle(64);
                        self.pending_debug_latents.push(latent);
                    } else {
                        game.vm
                            .resolve_latent(latent, soe_papyrus::Value::Float(-1.0));
                    }
                }
            }
        }
    }

    fn sync_debug_ref_markers(&mut self) {
        let Some(game) = &self.game else {
            return;
        };
        let Some(scenario) = self.base().get_world_3d().map(|world| world.get_scenario()) else {
            return;
        };
        let placements: BTreeMap<_, _> = {
            let world = game.world();
            world
                .debug_ref_positions()
                .into_iter()
                .filter_map(|id| world.ref_placement(id).map(|placement| (id, placement)))
                .collect()
        };
        let requested: BTreeSet<_> = game.world().debug_ref_positions().into_iter().collect();
        let mut server = godot::classes::RenderingServer::singleton();
        if !placements.is_empty() && self.debug_ref_marker_mesh.is_none() {
            let mut mesh = godot::classes::SphereMesh::new_gd();
            mesh.set_radius(4.0 * crate::units::METRES_PER_UNIT);
            mesh.set_height(8.0 * crate::units::METRES_PER_UNIT);
            let mut material = godot::classes::StandardMaterial3D::new_gd();
            material.set_shading_mode(godot::classes::base_material_3d::ShadingMode::UNSHADED);
            material.set_albedo(godot::builtin::Color::from_rgba(1.0, 0.15, 0.05, 1.0));
            material.set_feature(godot::classes::base_material_3d::Feature::EMISSION, true);
            material.set_emission(godot::builtin::Color::from_rgb(1.0, 0.05, 0.0));
            self.debug_ref_marker_mesh = Some(mesh);
            self.debug_ref_marker_material = Some(material);
        }
        let Some(mesh) = self.debug_ref_marker_mesh.as_ref() else {
            return;
        };
        let material = self.debug_ref_marker_material.as_ref();
        for (id, placement) in placements {
            let pos = crate::units::SkyrimPos::new(
                placement.position[0],
                placement.position[1],
                placement.position[2],
            )
            .to_godot();
            let transform = godot::builtin::Transform3D::new(
                godot::builtin::Basis::IDENTITY,
                godot::builtin::Vector3::new(pos.x, pos.y, pos.z),
            );
            let rid = *self.debug_ref_markers.entry(id).or_insert_with(|| {
                let rid = soe_trace::Profile::render_submission(|| {
                    server.instance_create2(mesh.get_rid(), scenario)
                });
                if let Some(material) = material {
                    server.instance_geometry_set_material_override(rid, material.get_rid());
                }
                rid
            });
            server.instance_set_transform(rid, transform);
        }
        let stale: Vec<_> = self
            .debug_ref_markers
            .keys()
            .filter(|id| !requested.contains(id))
            .copied()
            .collect();
        for id in stale {
            if let Some(rid) = self.debug_ref_markers.remove(&id) {
                server.free_rid(rid);
            }
        }
    }

    fn enter_main_menu(&mut self) {
        if self.main_menu.is_active() {
            return;
        }
        self.release_carry();
        self.close_menu();
        if let Some(streamer) = self.game_streamer.as_mut() {
            streamer.bind_mut().clear();
        }
        self.game = None;
        self.automatic_detection = super::detection::AutomaticDetection::default();
        self.clear_player_input();
        self.data_source = None;
        self.game_streamer = None;
        self.loaded_cell = None;
        self.exterior = None;
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        self.collision_references.clear();
        self.collision_bodies.clear();
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
        let _system = soe_trace::SystemScope::new(soe_trace::System::Streaming);
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
        let is_load_door = self.game.as_ref().is_some_and(|game| {
            game.world()
                .reference_ref(target)
                .is_some_and(|reference| reference.teleport.is_some())
        });
        if is_load_door {
            self.base_mut().emit_signal(
                "transition_requested",
                &[Variant::from("load_door"), Variant::from("activation")],
            );
        }
        let (activated, show_container, show_crafting, lockpick_request) = {
            let Some(game) = &mut self.game else {
                return false;
            };
            // A player pick must not interact with an invisible/deleted reference.
            // The shared game path handles locks and inventory keys.
            let eligible = game.world().reference_enabled(target);
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
        if activated {
            self.open_swing_door(target);
        }
        activated
    }

    pub(super) fn open_swing_door(&mut self, target: GlobalFormId) {
        let (Some(source), Some(game)) = (&self.data_source, &self.game) else {
            return;
        };
        let Some((base, position, rotation, scale, opened)) = ({
            let world = game.world();
            world.reference_ref(target).map(|reference| {
                (
                    reference.base,
                    reference.position,
                    reference.rotation,
                    reference.scale,
                    reference.opened,
                )
            })
        }) else {
            return;
        };
        if !opened {
            if let Some(mut node) = self.swing_doors.remove(&target) {
                node.queue_free();
            }
            return;
        }
        if let Some(node) = self.swing_doors.get_mut(&target) {
            node.bind_mut().open();
            return;
        }
        let Some(base) = base else {
            return;
        };
        let Some(base_record) = source.order().winner(base) else {
            return;
        };
        if base_record.tag != soe_esm::Tag::new(b"DOOR") {
            return;
        }
        let Some(reference_record) = source.order().winner(target) else {
            return;
        };
        if reference_record.find(soe_esm::Tag::new(b"XTEL")).is_some() {
            return;
        }
        let Some(model) = crate::data_source::model_for(base_record) else {
            return;
        };
        let Ok(bytes) = source.vfs().read(&model) else {
            return;
        };
        let packed: PackedByteArray = bytes.iter().copied().collect();
        let mut node = crate::godot_animated_static::AnimatedStaticNode::new_alloc();
        node.set_name(&format!("SwingDoor_{target}"));
        node.bind_mut().enable_collision();
        self.base_mut().add_child(&node);
        if !node.bind_mut().load_nif_bytes(packed) {
            node.queue_free();
            return;
        }
        let transform = placement_to_godot(
            soe_physics::Iso::placement(position, rotation, scale)
                .rotation
                .rows,
            position,
            scale,
        );
        node.set_transform(Transform3D::new(
            Basis::from_cols(
                Vector3::new(
                    transform.basis[0][0],
                    transform.basis[1][0],
                    transform.basis[2][0],
                ),
                Vector3::new(
                    transform.basis[0][1],
                    transform.basis[1][1],
                    transform.basis[2][1],
                ),
                Vector3::new(
                    transform.basis[0][2],
                    transform.basis[1][2],
                    transform.basis[2][2],
                ),
            ),
            Vector3::new(transform.origin.x, transform.origin.y, transform.origin.z),
        ));
        node.bind_mut().open();
        self.swing_doors.insert(target, node);
    }

    pub(super) fn clear_swing_doors(&mut self) {
        for (_, mut node) in self.swing_doors.drain() {
            node.queue_free();
        }
    }

    pub(super) fn sync_game_pose(&mut self) -> bool {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Streaming);
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
            soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: game player has no destination cell");
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
            self.release_carry();
            self.clear_swing_doors();
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
            let location = match prepared
                .map_or_else(|| source.load_location(cell, position, 1), Ok)
            {
                Ok(location) => location,
                Err(error) => {
                    soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
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
            let Some(game) = &self.game else {
                query_collision.free();
                return false;
            };
            if !streamer.is_instance_valid()
                || !streamer
                    .bind_mut()
                    .replace_location(&location, &game.world())
            {
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
                        soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
                        return false;
                    }
                }
            }
            if worldspace.is_none() {
                let mut node = streamer.bind_mut();
                node.clear_terrain_lod();
                node.clear_object_lod();
            }
            if let Some(worldspace) = worldspace {
                let mut planner = crate::streaming::CellStreamer::new(super::exterior::config())
                    .map_err(|error| {
                        soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
                        godot_error!("PlayerNode: {error}");
                    });
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
                if let Err(error) = game.attach_streaming_cells(&entered) {
                    soe_trace::error!(target: "host::gameplay", system = "host::gameplay", "PlayerNode: {error}");
                    return false;
                }
                self.exterior = Some(super::exterior::ExteriorRuntime {
                    world: worldspace,
                    planner: planner.clone(),
                    cells: location.collision_cells,
                    lod_started: false,
                });
            }
            let collision = location.collision;
            let mut world = collision.world;
            let id = world.create_character(CharacterSettings::default(), feet);
            self.world = Some(world);
            self.character = Some(id);
            self.collision_references = collision.references;
            self.rebuild_collision_bodies();
            self.collision_visibility.clear();
            self.collision_reset = true;
            self.loaded_cell = Some(cell);
            self.sync_cell_ambience(cell);
            self.sync_cell_music(cell);
            self.record_door_texture_preparation(texture_preparation);
            self.footstep_cadence.reset();
            self.motion = CameraMotion::new(feet, self.capsule_height());
            soe_trace::info!(target: "host::gameplay", system = "host::gameplay",
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
mod furniture_tests {
    use super::should_stand_from_furniture;
    use soe_esm::records::Transform;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
    use soe_game::Game;

    const T: fn(&[u8; 4]) -> Tag = Tag::new;
    const CHAIR: u32 = 0x200;
    const CHAIR_BASE: u32 = 0x300;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn game() -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        let furniture = Record::new(T(b"FURN"), CHAIR_BASE)
            .sub(T(b"MNAM"), 1_u32.to_le_bytes())
            .sub(T(b"FNPR"), [1_u8, 0, 0, 0]);
        plugin.push_top(furniture);
        plugin.push_top(
            Record::new(T(b"REFR"), CHAIR)
                .sub(T(b"NAME"), CHAIR_BASE.to_le_bytes())
                .sub(
                    T(b"DATA"),
                    Transform {
                        position: [25.0, 50.0, 75.0],
                        rotation: [0.0, 0.0, 1.5],
                    }
                    .to_bytes(),
                ),
        );
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let player = game.player();
        game.world_mut().reference(player).unwrap().is_actor = true;
        assert!(game.activate(id(CHAIR), player));
        game
    }

    #[test]
    fn hst_09_host_stands_after_movement_or_cell_change() {
        let mut game = game();
        let player = game.player();
        {
            let mut world = game.world_mut();
            assert!(!should_stand_from_furniture(&mut world, player));
            world.reference(player).unwrap().position[0] += 24.0;
            assert!(!should_stand_from_furniture(&mut world, player));
            world.reference(player).unwrap().position[0] += 0.1;
            assert!(should_stand_from_furniture(&mut world, player));
            assert!(world.stand_from_furniture(player));
            assert!(!world.furniture_marker_in_use(id(CHAIR), Some(0)));
        }

        assert!(game.activate(id(CHAIR), player));
        {
            let mut world = game.world_mut();
            world.reference(player).unwrap().cell = Some(id(9));
            assert!(should_stand_from_furniture(&mut world, player));
        }
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
        for axis in 0..3 {
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut position = [1.0, 2.0, 3.0];
                position[axis] = invalid;
                assert!(restored_player_destination(Some(cell), position, [0.0; 3]).is_none());

                let mut rotation = [0.0; 3];
                rotation[axis] = invalid;
                assert!(
                    restored_player_destination(Some(cell), [1.0, 2.0, 3.0], rotation).is_none()
                );
            }
        }
    }
}
