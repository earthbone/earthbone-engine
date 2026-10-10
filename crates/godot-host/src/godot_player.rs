// SPDX-License-Identifier: GPL-3.0-or-later

//! The node that turns input devices into a character walking
//! around a cell.
//!
//! Thin on purpose, the same way `godot_layer` is. Every rule lives in
//! `input` and `camera`, which have no engine dependency and carry the tests.
//! This file reads devices, steps the physics world the engine already owns,
//! and writes one camera transform.
//!
//! Two choices worth stating. The bindings are registered as real
//! `InputMap` actions under their vanilla event names, so a project or a
//! player can inspect and rebind them the way any Godot game allows, and the
//! control map read from the game's own `controlmap.txt` is what fills them.
//! And the character is `soe_physics`, not a Godot body: `docs/physics.md`
//! keeps the simulation's authority in the engine so the headless run and the
//! rendered one cannot drift. One Camera3D node also registers the listener
//! required by Godot's positional audio, which a camera RID alone cannot do.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use godot::builtin::Rid;
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{
    Camera3D, CanvasLayer, ColorRect, Engine, INode3D, Input, InputEventJoypadButton,
    InputEventJoypadMotion, InputEventKey, InputEventMouseButton, InputEventMouseMotion, InputMap,
    Node3D, RenderingServer, SphereMesh, StandardMaterial3D,
};
use godot::global::{JoyAxis, JoyButton, Key, MouseButton};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_physics::{BodyId, CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};

use crate::camera::{CameraHostState, CameraMode, CameraMotion, CameraRig, Eye};
use crate::data_source::DataSource;
use crate::door_fade::{DoorFade, GameFade};
use crate::input::devices::{godot_mouse_for, godot_pad_for, Devices, PadControl};
use crate::input::{godot_key_for, Action, Bindings, ControlMap, Driver, InputState};

mod carry;
mod containers;
mod detection;
mod exterior;
mod footsteps;
mod gameplay;
mod hud;
pub(super) mod item_card;
mod items;
mod lockpicking;
mod magic;
mod melee;
mod menu;
mod profiling;
mod quick_loot;
mod race;
mod reference_collision;
mod saves;
mod scripted;
mod simulation;
mod sky;
mod training;

/// How far ahead of the eye an activate ray reaches, in Skyrim units. The
/// vanilla reach is about this, and it is a setting rather than a constant so
/// a later pass can read `fActivatePickLength` from the INI.
const ACTIVATE_REACH: f32 = 180.0;

struct DoorFadeView {
    overlay: Gd<ColorRect>,
}

struct ImageSpaceTintView {
    overlay: Gd<ColorRect>,
}

/// Drives one character through a cell's collision and places the camera.
///
/// Call `load_data` then `enter_cell` from GDScript, and the node takes over
/// from there.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct PlayerNode {
    base: Base<Node3D>,
    /// Capture the mouse on ready. Off for a headless or scripted run, where
    /// grabbing the pointer would be rude.
    #[export]
    capture_mouse: bool,
    /// Step the simulation from `_physics_process`. Off lets a test drive
    /// `advance` by hand with a fixed step.
    #[export]
    auto_step: bool,
    /// Stick look rate in mouse units per second, before camera sensitivity.
    #[export]
    gamepad_look_speed: f32,
    /// Maximum exterior mesh triangles uploaded by one walking tick. Zero is unlimited.
    #[export]
    exterior_triangle_budget: i64,
    /// Maximum exterior vertex and index bytes uploaded by one walking tick.
    #[export]
    exterior_byte_budget: i64,
    host_frame: soe_trace::FrameSnapshot,
    profile_inputs: Vec<soe_trace::InputLatency>,
    profile_applied_inputs: Vec<soe_trace::InputLatency>,
    host_history: soe_trace::FrameHistory,
    last_physics_queries: soe_trace::PhysicsQueryStats,
    physics_ticks_since_process: u32,
    physics_advances_since_process: u32,
    last_physics_steps: u32,
    last_catchup_steps: u32,
    /// Set by automatic physics ticks, cleared by the one sync per frame in `process`.
    reference_sync_pending: bool,
    /// Godot's `max_physics_steps_per_frame` before gameplay lowered it.
    prior_max_physics_steps: Option<i32>,
    catchup_since: Option<std::time::Instant>,
    catchup_warning_sent: bool,
    scene_history: soe_trace::FrameHistory,
    scene_frame: soe_trace::FrameSample,
    scene_budget: soe_trace::FrameSample,
    budget_frame: Option<soe_trace::FrameSnapshot>,
    monitor_names: Vec<StringName>,
    /// Cell radius beyond which distant LOD replaces full resolution geometry.
    #[export]
    exterior_lod_full_res_radius: i32,
    /// Cell radius out to which terrain and object LOD are streamed.
    #[export]
    exterior_lod_radius: i32,
    /// Approximate resident mesh byte budget for each LOD stream. Zero is unlimited.
    #[export]
    exterior_lod_byte_budget: i64,
    rig: CameraRig,
    motion: CameraMotion,
    detection_time_usec: i64,
    detection_stage_usec: [i64; 4],
    automatic_detection: detection::AutomaticDetection,
    automatic_physics_step: bool,
    bindings: Bindings,
    driver: Driver,
    world: Option<SimpleWorld>,
    query_collision: crate::godot_physics::CellBodies,
    collision_references: HashMap<BodyId, GlobalFormId>,
    /// Reverse of `collision_references`, so one reference finds its bodies at once.
    collision_bodies: HashMap<GlobalFormId, Vec<BodyId>>,
    collision_changes: soe_game::world::ReferenceChanges,
    collision_cells: BTreeSet<GlobalFormId>,
    collision_residents: BTreeMap<GlobalFormId, GlobalFormId>,
    collision_visibility: std::collections::BTreeMap<GlobalFormId, bool>,
    /// Set when the collision world is replaced, so the next sync restarts tracking.
    collision_reset: bool,
    collision_pending_cells: std::collections::VecDeque<GlobalFormId>,
    collision_removing: std::collections::VecDeque<(GlobalFormId, usize)>,
    collision_expand_offset: usize,
    collision_pending_watch: std::collections::VecDeque<GlobalFormId>,
    collision_backlog: BTreeSet<GlobalFormId>,
    character: Option<BodyId>,
    data_source: Option<Arc<DataSource>>,
    game: Option<soe_game::Game>,
    simulation: Option<soe_game::simulation::Simulation>,
    simulation_pair: Option<Arc<soe_game::simulation::SnapshotPair>>,
    simulation_error: String,
    player_input: soe_game::player_input::Driver,
    animation_source: Option<GlobalFormId>,
    /// The existing skinned node bound to the player's own appearance.
    player_appearance: Option<Gd<crate::godot_actor::SkinnedActorNode>>,
    pending_debug_latents: Vec<soe_papyrus::vm::LatentId>,
    debug_ref_markers: BTreeMap<GlobalFormId, Rid>,
    debug_ref_marker_mesh: Option<Gd<SphereMesh>>,
    debug_ref_marker_material: Option<Gd<StandardMaterial3D>>,
    melee_contact: crate::combat::MeleeContact,
    melee_bodies: HashMap<Rid, GlobalFormId>,
    aimed_projectile_contacts: crate::combat::AimedProjectileContacts,
    /// One server body per loaded actor, rebuilt from the shared World on
    /// each physics tick. No scene node is allocated per reference.
    actor_bodies: crate::godot_physics::ActorBodies,
    /// The form the last HitFrame contact query struck, for diagnostics.
    last_melee_target: Option<GlobalFormId>,
    last_melee_obstruction: Option<Rid>,
    image_space: crate::image_space::ImageSpaceStack,
    image_space_renderer: crate::image_space::renderer::BaseRenderer,
    image_space_modifier_renderer: crate::image_space::renderer::ModifierRenderer,
    image_space_tint: Option<ImageSpaceTintView>,
    presented_hud_cart_mode: bool,
    effect_shaders: crate::effect_shader::EffectShaderStack,
    sound_bank: Option<Gd<crate::godot_audio::SoundBank>>,
    cell_music_type: Option<GlobalFormId>,
    dialogue_voice_slot: Option<i32>,
    ambient_response: Option<soe_game::dialogue::Line>,
    footstep_cadence: crate::audio::FootstepCadence,
    footstep_count: u64,
    footstep_errors: u64,
    game_streamer: Option<Gd<crate::godot_layer::CellStreamerNode>>,
    loaded_cell: Option<GlobalFormId>,
    last_work_counts: soe_trace::WorkCounts,
    door_fade: DoorFade,
    game_fade: GameFade,
    door_fade_view: Option<DoorFadeView>,
    hud_view: Option<hud::HudView>,
    item_card_view: Option<item_card::ItemCardView>,
    quick_loot: crate::quick_loot::QuickLoot,
    quick_loot_view: Option<crate::quick_loot::QuickLootView>,
    exterior: Option<exterior::ExteriorRuntime>,
    interior_prefetch: crate::interior_prefetch::InteriorPrefetch,
    interior_prefetch_elapsed: f64,
    camera: Option<Gd<Camera3D>>,
    camera_state: CameraHostState,
    camera_shake: Option<(f32, f32, f32)>,
    camera_shake_time: f32,
    /// Mouse motion accumulated since the last camera or physics update, so a frame that
    /// delivers two motion events turns twice as far rather than once.
    pending_look: (f32, f32),
    /// Whether the activate ray hit something on the last step, reported to
    /// GDScript so a prompt can be drawn without a second raycast.
    last_activate_hit: bool,
    last_activate_reference: Option<GlobalFormId>,
    grabbed_host: Option<carry::HeldReference>,
    activate_hold: crate::carry::ActivateHold,
    /// Distance from the eye to the host body held by Telekinesis.
    telekinesis_distance: Option<f32>,
    host_telekinesis_target: Option<GlobalFormId>,
    /// The live Grab Actor effect and actor currently driven by the host.
    host_grab_actor: Option<(soe_game::world::magic::EffectId, GlobalFormId)>,
    pending_lockpick: Option<GlobalFormId>,
    action_latches: BTreeMap<Action, u64>,
    held_actions: BTreeSet<Action>,
    devices: Devices,
    input_focused: bool,
    menu: crate::ui::MenuState,
    training_trainer: Option<GlobalFormId>,
    main_menu: crate::ui::MainMenuState,
    inventory_category: crate::inventory_menu::Category,
    magic_hand: soe_game::combat::Hand,
    race_choice: Option<(GlobalFormId, bool)>,
    appearance_choice: Option<soe_game::appearance::PlayerAppearance>,
    menu_view: Option<menu::MenuView>,
    interface_cache: Option<String>,
    quest_journal_snapshot: Option<Vec<crate::quest_journal::Entry>>,
    objective_map_snapshot: Option<Vec<crate::objective_map::Marker>>,
    mcm_config: Option<GlobalFormId>,
    container_menu: Option<(GlobalFormId, crate::inventory_menu::Category)>,
    barter_menu: Option<(GlobalFormId, GlobalFormId)>,
    crafting_bench: Option<GlobalFormId>,
    lockpick_target: Option<GlobalFormId>,
    lockpick_angle: f32,
    lockpick_status: Option<Gd<godot::classes::Label>>,
    restore_mouse_on_focus: Option<godot::classes::input::MouseMode>,
}

#[godot_api]
impl INode3D for PlayerNode {
    fn init(base: Base<Node3D>) -> Self {
        PlayerNode {
            base,
            capture_mouse: true,
            auto_step: true,
            gamepad_look_speed: 800.0,
            exterior_triangle_budget: 0,
            exterior_byte_budget: 0,
            host_frame: soe_trace::FrameSnapshot::now(),
            profile_inputs: Vec::new(),
            profile_applied_inputs: Vec::new(),
            host_history: soe_trace::FrameHistory::default(),
            last_physics_queries: soe_trace::PhysicsQueryStats {
                counts: [[0; soe_trace::SYSTEM_COUNT]; 3],
                elapsed_ns: [[0; soe_trace::SYSTEM_COUNT]; 3],
            },
            physics_ticks_since_process: 0,
            physics_advances_since_process: 0,
            last_physics_steps: 0,
            last_catchup_steps: 0,
            reference_sync_pending: false,
            prior_max_physics_steps: None,
            catchup_since: None,
            catchup_warning_sent: false,
            scene_history: soe_trace::FrameHistory::default(),
            scene_frame: crate::scene_timing::snapshot(),
            scene_budget: soe_trace::FrameSample::default(),
            budget_frame: None,
            monitor_names: Vec::new(),
            exterior_lod_full_res_radius: 1,
            exterior_lod_radius: 16,
            exterior_lod_byte_budget: 0,
            rig: CameraRig::default(),
            detection_time_usec: 0,
            detection_stage_usec: [0; 4],
            automatic_detection: detection::AutomaticDetection::default(),
            automatic_physics_step: false,
            motion: CameraMotion::new(Vec3::ZERO, CharacterSettings::default().height),
            bindings: Bindings::default(),
            driver: Driver::new(),
            world: None,
            query_collision: crate::godot_physics::CellBodies::default(),
            collision_references: HashMap::new(),
            collision_bodies: HashMap::new(),
            collision_changes: soe_game::world::ReferenceChanges::default(),
            collision_cells: BTreeSet::new(),
            collision_residents: BTreeMap::new(),
            collision_visibility: std::collections::BTreeMap::new(),
            collision_reset: true,
            collision_pending_cells: std::collections::VecDeque::new(),
            collision_removing: std::collections::VecDeque::new(),
            collision_expand_offset: 0,
            collision_pending_watch: std::collections::VecDeque::new(),
            collision_backlog: BTreeSet::new(),
            character: None,
            data_source: None,
            game: None,
            simulation: None,
            simulation_pair: None,
            simulation_error: String::new(),
            player_input: soe_game::player_input::Driver::default(),
            animation_source: None,
            player_appearance: None,
            pending_debug_latents: Vec::new(),
            debug_ref_markers: BTreeMap::new(),
            debug_ref_marker_mesh: None,
            debug_ref_marker_material: None,
            melee_contact: crate::combat::MeleeContact::default(),
            melee_bodies: HashMap::new(),
            aimed_projectile_contacts: crate::combat::AimedProjectileContacts::default(),
            actor_bodies: crate::godot_physics::ActorBodies::default(),
            last_melee_target: None,
            last_melee_obstruction: None,
            image_space: crate::image_space::ImageSpaceStack::default(),
            image_space_renderer: crate::image_space::renderer::BaseRenderer::default(),
            image_space_modifier_renderer: crate::image_space::renderer::ModifierRenderer::default(
            ),
            image_space_tint: None,
            presented_hud_cart_mode: false,
            effect_shaders: crate::effect_shader::EffectShaderStack::default(),
            sound_bank: None,
            cell_music_type: None,
            dialogue_voice_slot: None,
            ambient_response: None,
            footstep_cadence: crate::audio::FootstepCadence::default(),
            footstep_count: 0,
            footstep_errors: 0,
            game_streamer: None,
            loaded_cell: None,
            last_work_counts: soe_trace::WorkCounts::default(),
            door_fade: DoorFade::default(),
            game_fade: GameFade::default(),
            door_fade_view: None,
            exterior: None,
            interior_prefetch: crate::interior_prefetch::InteriorPrefetch::default(),
            interior_prefetch_elapsed: 0.0,
            camera: None,
            camera_state: CameraHostState::default(),
            camera_shake: None,
            camera_shake_time: 0.0,
            pending_look: (0.0, 0.0),
            last_activate_hit: false,
            last_activate_reference: None,
            grabbed_host: None,
            activate_hold: crate::carry::ActivateHold::default(),
            telekinesis_distance: None,
            host_telekinesis_target: None,
            host_grab_actor: None,
            pending_lockpick: None,
            action_latches: BTreeMap::new(),
            held_actions: BTreeSet::new(),
            devices: Devices::default(),
            input_focused: true,
            menu: crate::ui::MenuState::default(),
            training_trainer: None,
            main_menu: crate::ui::MainMenuState::default(),
            inventory_category: crate::inventory_menu::Category::All,
            magic_hand: soe_game::combat::Hand::Right,
            race_choice: None,
            appearance_choice: None,
            menu_view: None,
            interface_cache: None,
            quest_journal_snapshot: None,
            objective_map_snapshot: None,
            mcm_config: None,
            container_menu: None,
            barter_menu: None,
            crafting_bench: None,
            lockpick_target: None,
            lockpick_angle: 0.0,
            lockpick_status: None,
            restore_mouse_on_focus: None,
            hud_view: None,
            item_card_view: None,
            quick_loot: crate::quick_loot::QuickLoot::default(),
            quick_loot_view: None,
        }
    }

    fn ready(&mut self) {
        soe_trace::Profile::from_environment();
        let directory =
            godot::classes::ProjectSettings::singleton().globalize_path("user://hitches");
        soe_trace::Trace::set_hitch_directory(std::path::Path::new(&directory.to_string()));
        self.register_system_monitors();
        RenderingServer::singleton().connect(
            "frame_post_draw",
            &self.base().callable("profile_frame_presented"),
        );
        if let Some(mut window) = self.base().get_window() {
            window.connect("focus_exited", &self.base().callable("input_focus_lost"));
            window.connect("focus_entered", &self.base().callable("input_focus_gained"));
        }
        Input::singleton().connect(
            "joy_connection_changed",
            &self.base().callable("joy_connection_changed"),
        );
        self.register_actions();
        self.create_camera();
        self.create_image_space_tint();
        self.create_door_fade();
        self.create_hud();

        self.create_menu();
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::CAPTURED);
        }
    }

    fn unhandled_input(&mut self, event: Gd<godot::classes::InputEvent>) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Input);
        if !self.input_focused || self.menu.is_open() {
            return;
        }
        if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
            let rel = motion.get_relative();
            self.pending_look.0 += rel.x;
            self.pending_look.1 += rel.y;
        }
    }

    fn input(&mut self, event: Gd<godot::classes::InputEvent>) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Input);
        let latency = event
            .clone()
            .try_cast::<InputEventKey>()
            .ok()
            .and_then(|key| {
                soe_trace::Profile::input(|| {
                    format!(
                        "key_{}_{}",
                        key.get_keycode().ord(),
                        if key.is_pressed() {
                            "pressed"
                        } else {
                            "released"
                        }
                    )
                })
            });
        let _system = soe_trace::SystemScope::new(soe_trace::System::Input);
        if !self.input_focused {
            return;
        }
        if self.quick_loot_input(&event) {
            if let Some(mut viewport) = self.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
            if key.get_keycode() == Key::I
                && key.is_pressed()
                && !key.is_echo()
                && !self.menu.is_open()
                && self.open_inventory_menu()
            {
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
                if let Some(latency) = latency {
                    self.profile_applied_inputs.push(latency);
                }
                return;
            }
            if key.get_keycode() == Key::M
                && key.is_pressed()
                && !key.is_echo()
                && !self.menu.is_open()
                && self.open_magic_menu()
            {
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
                if let Some(latency) = latency {
                    self.profile_applied_inputs.push(latency);
                }
                return;
            }
            if key.get_keycode() == Key::J
                && key.is_pressed()
                && !key.is_echo()
                && !self.menu.is_open()
                && self.open_quest_journal()
            {
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
                if let Some(latency) = latency {
                    self.profile_applied_inputs.push(latency);
                }
                return;
            }
            if key.get_keycode() == Key::ESCAPE && key.is_pressed() && !key.is_echo() {
                if self.menu.is_open() {
                    self.close_menu();
                } else {
                    self.open_menu("Paused".into());
                }
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
                if let Some(latency) = latency {
                    self.profile_applied_inputs.push(latency);
                }
                return;
            }
        }
        if self.menu.is_open() {
            return;
        }
        if let Some(latency) = latency {
            self.profile_inputs.push(latency);
        }
        if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
            self.devices.key(
                i64::from(key.get_keycode().ord()),
                key.is_pressed(),
                &self.bindings,
            );
        } else if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            self.devices.mouse_button(
                button.get_button_index().ord(),
                button.is_pressed(),
                &self.bindings,
            );
        } else if let Ok(button) = event.clone().try_cast::<InputEventJoypadButton>() {
            self.devices.pad_button(
                button.get_device(),
                button.get_button_index().ord(),
                button.is_pressed(),
                &self.bindings,
            );
        } else if let Ok(axis) = event.try_cast::<InputEventJoypadMotion>() {
            self.devices.pad_axis(
                axis.get_device(),
                axis.get_axis().ord(),
                axis.get_axis_value(),
                &self.bindings,
            );
        }
    }

    fn physics_process(&mut self, delta: f64) {
        self.physics_ticks_since_process = self.physics_ticks_since_process.saturating_add(1);
        if self.auto_step {
            let frame = Engine::singleton().get_process_frames();
            if let Some((elapsed, motion)) = self.automatic_detection.before_step(frame) {
                self.advance_detection(elapsed, motion);
            }
            self.automatic_physics_step = true;
            self.advance(delta);
            self.automatic_physics_step = false;
            self.profile_applied_inputs.append(&mut self.profile_inputs);
            if self.simulation.is_some() {
                self.sync_simulation_bodies();
            } else {
                self.sync_actor_bodies();
                self.update_grab_actor();
            }
        }
    }

    fn process(&mut self, delta: f64) {
        let engine_scope = soe_trace::SystemScope::new(soe_trace::System::Other);
        self.present_simulation();
        self.flush_reference_sync();
        self.last_physics_queries = soe_trace::physics_query_stats();
        soe_trace::reset_physics_queries();
        self.last_physics_steps = std::mem::take(&mut self.physics_ticks_since_process)
            .max(std::mem::take(&mut self.physics_advances_since_process));
        self.last_catchup_steps = self.last_physics_steps.saturating_sub(1);
        if self.last_catchup_steps > 0 {
            let since = self
                .catchup_since
                .get_or_insert_with(std::time::Instant::now);
            if !self.catchup_warning_sent && since.elapsed() >= std::time::Duration::from_secs(1) {
                godot::global::godot_warn!(
                    "Physics catch-up steps repeated for at least one second"
                );
                self.catchup_warning_sent = true;
            }
        } else {
            self.catchup_since = None;
            self.catchup_warning_sent = false;
        }
        if self
            .sound_bank
            .as_ref()
            .is_some_and(|bank| bank.bind().music_needs_selection())
        {
            if let Some(cell) = self.loaded_cell {
                self.sync_cell_music(cell);
            }
        }
        let render_scope = soe_trace::SystemScope::new(soe_trace::System::Render);
        let scene_scope = crate::scene_timing::Scope::new(soe_trace::System::Other);
        if let Some(view) = self.item_card_view.as_mut() {
            view.tick(delta);
        }
        self.update_quick_loot();
        if let Some(view) = self.quick_loot_view.as_mut() {
            view.tick(delta);
        }
        if self.menu.title() == Some("Quests") {
            self.refresh_quest_journal();
        }
        if self.menu.title() == Some("Map") {
            self.refresh_objective_map();
        }
        if self.auto_step && !self.menu.is_open() {
            self.look_stick(delta);
            let (dx, dy) = std::mem::take(&mut self.pending_look);
            self.rig.look(dx, dy);
            let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
            let (feet, height) = self.motion.sample(fraction);
            self.update_camera_shake(delta as f32);
            self.place_camera(feet, height);
        }
        if let Some(game) = self.game.as_mut() {
            if let Some(view) = self.hud_view.as_mut() {
                view.refresh(game, self.rig.yaw());
                view.set_cart_mode(game.world().hud_cart_mode);
            }
        } else if let Some(view) = self.hud_view.as_mut() {
            view.hide();
        }
        if let Some(view) = self.hud_view.as_mut() {
            view.tick_presentation(delta);
        }
        if self.auto_step {
            self.refresh_image_space();
        }
        drop(scene_scope);
        drop(render_scope);
        drop(engine_scope);
        let sample = self.host_frame.finish();
        soe_trace::Profile::systems("host", &sample);
        // CONTEXT: The host frame must complete the hitch tail before subphase history can export it.
        self.host_history.record(sample);
        self.scene_history
            .record(crate::scene_timing::elapsed(&self.scene_frame));
        self.scene_frame = crate::scene_timing::snapshot();
        crate::godot_profiler::publish(&self.host_history);
        self.host_frame = soe_trace::FrameSnapshot::now();
    }

    fn exit_tree(&mut self) {
        if let Err(error) = soe_trace::Trace::wait_for_hitch_exports() {
            godot_error!("hitch export: {error}");
        }
        self.release_carry();
        self.restore_catchup_cap();
        self.simulation = None;
        self.simulation_pair = None;
        self.remove_system_monitors();
        RenderingServer::singleton().disconnect(
            "frame_post_draw",
            &self.base().callable("profile_frame_presented"),
        );
        let mut server = RenderingServer::singleton();
        for (_, rid) in std::mem::take(&mut self.debug_ref_markers) {
            server.free_rid(rid);
        }
        self.debug_ref_marker_mesh = None;
        self.debug_ref_marker_material = None;
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        self.query_collision.free();
        self.actor_bodies.free();

        self.close_menu();
        self.quick_loot.close();
        self.quick_loot_view = None;
        self.menu_view = None;
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::VISIBLE);
        }
        self.camera = None;
        self.hud_view = None;
    }
}

#[godot_api]
impl PlayerNode {
    /// Arm the player's right-hand swing from the gathered attack press.
    /// The live node and host tests use this same route into the shared Game.
    pub fn arm_melee_swing_from_input(
        game: &mut soe_game::Game,
        contact: &mut crate::combat::MeleeContact,
        pressed: bool,
    ) -> Result<bool, soe_game::combat::Error> {
        contact.begin_from_attack_press(game, pressed)
    }

    /// Emitted after opening through gdext's reentrant base guard.
    #[signal]
    fn menu_opened(title: GString);

    /// Emitted after closing and releasing owned content controls.
    #[signal]
    fn menu_closed();

    /// Emitted before a visible transition starts so presentation can time its first drawn frame.
    #[signal]
    fn transition_requested(kind: GString, name: GString);

    /// A Game native changed the HUD cart-mode flag.
    #[signal]
    fn hud_cart_mode_changed(enabled: bool);

    /// A Game native requested the race and sex editor.
    #[signal]
    fn race_menu_requested();

    /// A Game native requested a screen blood effect.
    #[signal]
    fn screen_blood_requested(value: i32);

    /// A Game native requested Bink playback with its authored options.
    #[signal]
    fn bink_requested(
        file_name: GString,
        interruptible: bool,
        mute_audio: bool,
        mute_music: bool,
        letterbox: bool,
    );

    #[func]
    fn input_focus_lost(&mut self) {
        self.input_focused = false;
        if let Some(game) = self.game.as_mut() {
            let player = game.player();
            let _ = game.end_hand_cast(player, soe_game::combat::Hand::Left);
            let _ = game.end_hand_cast(player, soe_game::combat::Hand::Right);
        }
        self.release_carry();
        self.release_host_telekinesis();
        self.devices = Devices::default();
        self.pending_look = (0.0, 0.0);
        self.action_latches.clear();
        self.held_actions.clear();
        let mut input = Input::singleton();
        for action in Action::all() {
            input.action_release(action.event_name());
        }
    }

    #[func]
    fn input_focus_gained(&mut self) {
        self.input_focus_lost();
        self.input_focused = true;
        if !self.menu.is_open() {
            if let Some(mode) = self.restore_mouse_on_focus.take() {
                Input::singleton().set_mouse_mode(mode);
            }
        }
        // Godot can retain just-pressed flags until the frame ends.
        let until = Engine::singleton().get_process_frames() + 1;
        self.action_latches = Action::all().into_iter().map(|a| (a, until)).collect();
    }

    /// Emitted after gameplay is released and the host enters the main menu.
    #[signal]
    fn returned_to_main_menu();

    /// Emitted when the host starts a named title sequence on the main menu.
    #[signal]
    fn title_sequence_started(name: GString);

    /// Report whether PlayerNode has released gameplay and returned to its main menu state.
    #[func]
    pub fn is_main_menu(&self) -> bool {
        self.main_menu.is_active()
    }

    /// Return the currently playing title sequence name, if one was requested.
    #[func]
    pub fn active_title_sequence(&self) -> GString {
        self.main_menu
            .title_sequence()
            .map_or_else(GString::new, GString::from)
    }

    /// Invoke a main-menu Game native for host integration checks.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn debug_invoke_main_menu_native(&mut self, name: GString, sequence: GString) -> bool {
        self.invoke_main_menu_native_for_check(&name, &sequence)
    }

    /// Connect an already loaded character rig to this gameplay world's live
    /// skin and equipment records. Reuses the existing character node.
    #[func]
    pub fn bind_actor_appearance(
        &mut self,
        mut visual: Gd<crate::godot_actor::SkinnedActorNode>,
        file: i32,
        object: i64,
    ) -> bool {
        let (Some(game), Some(source)) = (self.game.as_mut(), self.data_source.as_ref()) else {
            return false;
        };
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let actor = soe_esm::GlobalFormId { file, object };
        let is_player = actor == game.player();
        let appearance =
            visual
                .bind_mut()
                .bind_appearance(game.shared_world(), Arc::clone(source), actor);
        match appearance {
            Ok(()) => {
                self.animation_source = Some(actor);
                visual.connect(
                    "animation_event",
                    &self.base().callable("report_animation_event"),
                );
                visual.connect(
                    "animation_graph_unloaded",
                    &self.base().callable("report_animation_graph_unloaded"),
                );
                if is_player {
                    visual.bind_mut().set_first_person_geometry_visible(
                        self.camera_state
                            .rendered_first_person_geometry_visible(self.rig.mode),
                    );
                    self.player_appearance = Some(visual);
                }
                true
            }
            Err(error) => {
                soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode actor appearance: {error}");
                false
            }
        }
    }

    /// Queue the same source-to-actor impulse used by ObjectReference.PushActorAway.
    ///
    /// Returns false if the game, either form ID, or the world request is invalid.
    #[func]
    pub fn push_actor_away(
        &mut self,
        source_file: i32,
        source_object: i64,
        target_file: i32,
        target_object: i64,
        force: f32,
    ) -> bool {
        let (Some(game), Ok(source_file), Ok(source_object), Ok(target_file), Ok(target_object)) = (
            self.game.as_mut(),
            u16::try_from(source_file),
            u32::try_from(source_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return false;
        };
        game.world_mut()
            .push_actor_away(
                soe_esm::GlobalFormId {
                    file: source_file,
                    object: source_object,
                },
                soe_esm::GlobalFormId {
                    file: target_file,
                    object: target_object,
                },
                force,
            )
            .is_ok()
    }

    /// Queue the same area impulse used by ObjectReference.KnockAreaEffect.
    ///
    /// Returns false if the game, source form ID, or world request is invalid.
    #[func]
    pub fn knock_area_effect(
        &mut self,
        source_file: i32,
        source_object: i64,
        magnitude: f32,
        radius: f32,
    ) -> bool {
        let (Some(game), Ok(source_file), Ok(source_object)) = (
            self.game.as_mut(),
            u16::try_from(source_file),
            u32::try_from(source_object),
        ) else {
            return false;
        };
        game.world_mut()
            .knock_area_effect(
                soe_esm::GlobalFormId {
                    file: source_file,
                    object: source_object,
                },
                magnitude,
                radius,
            )
            .is_ok()
    }

    /// Set an actor's explicit CMB-38 ragdoll mode in the shared world.
    ///
    /// Returns false if the game, form ID, or actor is invalid.
    #[func]
    pub fn force_actor_ragdoll(&mut self, file: i32, object: i64, enabled: bool) -> bool {
        let (Some(game), Ok(file), Ok(object)) = (
            self.game.as_mut(),
            u16::try_from(file),
            u32::try_from(object),
        ) else {
            return false;
        };
        game.world_mut()
            .force_ragdoll(soe_esm::GlobalFormId { file, object }, enabled)
            .is_ok()
    }

    /// Forward one authored behavior event from the bound actor rig.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    fn report_animation_event(&mut self, name: GString) {
        if let (Some(game), Some(source)) = (&mut self.game, self.animation_source) {
            game.report_animation_event(source, &name.to_string());
            game.pump();
            game.run_until_idle(64);
        }
    }

    /// Complete animation-event waits when the bound rig leaves the tree.
    #[func]
    fn report_animation_graph_unloaded(&mut self) {
        if let (Some(game), Some(source)) = (&mut self.game, self.animation_source) {
            game.animation_graph_unloaded(source);
            game.pump();
            game.run_until_idle(64);
        }
    }

    /// Clear a disconnected pad's buttons and axes before the next step.
    #[func]
    fn joy_connection_changed(&mut self, device: i32, connected: bool) {
        if !connected {
            self.devices.disconnect(device, &self.bindings);
        }
    }

    /// Open the game data this node reads cells from. Mirrors
    /// `CellStreamerNode::load_data`, and the two can point at the same
    /// install independently.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_data(&mut self, path: GString) -> bool {
        self.clear_player_input();
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        match DataSource::open(Path::new(&path.to_string())) {
            Ok(source) => {
                self.load_control_map(&source);
                if self.base().is_node_ready() {
                    self.register_actions();
                }
                self.interface_cache = None;
                self.data_source = Some(Arc::new(source));
                self.game = None;
                self.image_space = crate::image_space::ImageSpaceStack::default();
                self.refresh_image_space();
                self.effect_shaders = crate::effect_shader::EffectShaderStack::default();
                self.exterior = None;
                self.game_streamer = None;
                true
            }
            Err(e) => {
                soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode::load_data: {e}");
                self.data_source = None;
                self.game = None;
                self.image_space = crate::image_space::ImageSpaceStack::default();
                self.refresh_image_space();
                false
            }
        }
    }

    /// Share the streamer's current load order and VFS without reopening data.
    /// The source remains alive if the streamer leaves the tree or reloads.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn use_streamer_data(
        &mut self,
        streamer: Gd<crate::godot_layer::CellStreamerNode>,
    ) -> bool {
        let Some(source) = streamer.bind().shared_source() else {
            return false;
        };
        self.load_control_map(&source);
        if self.base().is_node_ready() {
            self.register_actions();
        }
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        self.interface_cache = None;
        self.data_source = Some(source);
        self.game = None;
        self.clear_player_input();
        self.image_space = crate::image_space::ImageSpaceStack::default();
        self.refresh_image_space();
        self.effect_shaders = crate::effect_shader::EffectShaderStack::default();
        self.exterior = None;
        self.game_streamer = Some(streamer);
        true
    }

    /// Replace device bindings with defaults plus a control-map overlay.
    /// Refreshes InputMap even when called after the node entered the tree.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn apply_control_map(&mut self, text: GString) {
        self.bindings = Bindings::default();
        self.bindings
            .apply_control_map(&ControlMap::parse(&text.to_string()));
        if self.base().is_node_ready() {
            self.register_actions();
        }
    }

    /// Build a cell's static collision and put the character in it at a
    /// position in Skyrim units. Answers how many bodies the world holds, or
    /// -1 on any failure.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn enter_cell(&mut self, editor_id: GString, x: f32, y: f32, z: f32) -> i32 {
        let Some(source) = self.data_source.as_ref() else {
            soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode::enter_cell: load_data was not called");
            return -1;
        };
        let Some(cell_id) = source.resolve_cell(&editor_id.to_string()) else {
            soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode::enter_cell: no cell named {editor_id}");
            return -1;
        };
        let collision = match source.collide_cell_id(cell_id) {
            Ok(c) => c,
            Err(e) => {
                soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode::enter_cell: {e}");
                return -1;
            }
        };
        let Some(query_collision) = self.prepare_query_collision(&collision) else {
            return -1;
        };
        self.query_collision.free();
        self.query_collision = query_collision;
        let mut world = collision.world;
        let bodies = collision.stats.bodies;
        let id = world.create_character(CharacterSettings::default(), Vec3::new(x, y, z));
        self.character = Some(id);
        self.world = Some(world);
        self.loaded_cell = Some(cell_id);
        self.door_fade = DoorFade::default();
        self.game_fade = GameFade::default();
        self.refresh_door_fade();
        self.interior_prefetch.invalidate();
        self.melee_contact.cancel();
        self.melee_bodies.clear();
        self.exterior = None;
        self.collision_references = collision.references;
        self.rebuild_collision_bodies();
        self.collision_visibility.clear();
        self.collision_reset = true;
        self.last_activate_hit = false;
        self.last_activate_reference = None;
        self.release_carry();
        if let Some(game) = &mut self.game {
            game.world_mut().set_grabbed_reference(None);
            game.world_mut().set_crosshair_reference(None);
        }
        self.motion = CameraMotion::new(self.feet(), self.capsule_height());
        self.update_camera();
        bodies as i32
    }

    /// One simulation step. Public so a scripted run can drive a fixed step
    /// instead of relying on frame timing.
    #[func]
    pub fn advance(&mut self, delta: f64) {
        if !self.automatic_physics_step {
            self.flush_automatic_detection();
            self.detection_time_usec = -1;
        }
        self.physics_advances_since_process = self.physics_advances_since_process.saturating_add(1);
        let _system = soe_trace::SystemScope::new(soe_trace::System::Other);
        let frame = soe_trace::WorkFrame::begin();
        self.advance_measured(delta);
        self.last_physics_queries = soe_trace::physics_query_stats();
        self.last_work_counts = frame.counts();
    }

    /// Operation counts from the last advance, including game and host queries.
    #[func]
    pub fn work_counts(&self) -> VarDictionary {
        crate::godot_layer::work_dictionary(&self.last_work_counts)
    }

    fn advance_measured(&mut self, delta: f64) {
        if self.simulation.is_some() {
            self.advance_simulation_controller(delta);
            return;
        }
        if self.player_input.pending() > 0 {
            self.step_player_input();
            return;
        }
        if !self.menu.is_open()
            && self
                .game
                .as_ref()
                .is_some_and(|game| game.gift_menu_request().is_some())
        {
            self.open_gift_menu();
        }
        if self.menu.is_open() {
            self.release_carry();
            if let Some(game) = &mut self.game {
                self.menu.tick(game, delta);
            }
            return;
        }
        let pending_location = self.game.as_mut().is_some_and(|game| {
            let player = game.player();
            game.world()
                .reference_ref(player)
                .is_some_and(|r| r.cell != self.loaded_cell)
        });
        if pending_location && !self.sync_game_pose() {
            self.auto_step = false;
            return;
        }
        if !self.auto_step {
            self.look_stick(delta);
        }
        let (dx, dy) = std::mem::take(&mut self.pending_look);
        self.rig.look(dx, dy);

        let input_scope = soe_trace::SystemScope::new(soe_trace::System::Input);
        let previous_held = self.held_actions.clone();
        let input = self.gather();
        drop(input_scope);
        self.route_hand_casts(
            magic::HandCastInput {
                was_held: previous_held.contains(&Action::LeftAttackBlock),
                held: input.left_attack_held,
                pressed: input.left_attack_pressed,
            },
            magic::HandCastInput {
                was_held: previous_held.contains(&Action::RightAttackBlock),
                held: input.right_attack_held,
                pressed: input.right_attack_pressed,
            },
        );
        if self.grabbed_host.is_none() {
            self.update_telekinesis(input.activate_pressed);
        }
        if input.toggle_pov_pressed {
            self.rig.mode = self.rig.mode.toggled();
        }
        self.begin_melee_swing_from_input(input.right_attack_pressed);

        let camera = self.rig;
        let intent = self.driver.update(&input, &camera);
        let before = self.feet();
        let was_grounded = self.character().is_some_and(|c| c.on_ground);
        let paralyzed = self.game.as_mut().is_some_and(|game| {
            let player = game.player();
            game.world().actor_is_paralyzed(player)
        });

        let physics_scope = soe_trace::SystemScope::new(soe_trace::System::Physics);
        if let (Some(world), Some(id)) = (self.world.as_mut(), self.character) {
            world.set_crouch(id, intent.crouch);
            let (direction, speed, jump) = player_locomotion(intent, paralyzed);
            world.walk(id, direction, speed);
            if jump {
                world.try_jump(id);
            }
            world.step(delta as f32);
        }

        drop(physics_scope);
        self.advance_footsteps(before, was_grounded, &input);

        self.update_carry_input(&input, delta);

        self.advance_game(delta, intent.sprinting);
        let player_motion = if input.is_moving() {
            if input.sprint || self.driver.always_run != input.walk {
                soe_game::detection::Motion::Running
            } else {
                soe_game::detection::Motion::Walking
            }
        } else {
            soe_game::detection::Motion::Still
        };
        if self.automatic_physics_step {
            self.automatic_detection.push(delta, player_motion);
        } else {
            self.advance_detection(delta, player_motion);
        }
        self.advance_door_fade(delta);

        self.motion.step(self.feet(), self.capsule_height());
        if !self.auto_step {
            self.update_camera();
        }
    }

    /// The character's feet in Skyrim units, as `(x, y, z)`.
    #[func]
    pub fn position(&self) -> Vector3 {
        let p = self.feet();
        Vector3::new(p.x, p.y, p.z)
    }

    /// Yaw and pitch in radians, Skyrim's convention.
    #[func]
    pub fn facing(&self) -> Vector2 {
        Vector2::new(self.rig.yaw(), self.rig.pitch())
    }

    #[func]
    pub fn is_third_person(&self) -> bool {
        self.rig.mode == CameraMode::Third
    }

    #[func]
    pub fn is_sneaking(&self) -> bool {
        self.driver.sneaking
    }

    #[func]
    pub fn is_on_ground(&self) -> bool {
        self.character().is_some_and(|c| c.on_ground)
    }

    #[func]
    fn footstep_count(&self) -> i64 {
        self.footstep_count as i64
    }

    #[func]
    fn footstep_errors(&self) -> i64 {
        self.footstep_errors as i64
    }

    /// Whether a cold door preparation currently leaves the fade visible.
    #[func]
    pub fn door_fade_active(&self) -> bool {
        self.door_fade.is_active()
    }

    /// Most recent door texture preparation in milliseconds.
    #[func]
    pub fn last_texture_preparation_ms(&self) -> f64 {
        self.door_fade.last_preparation().as_secs_f64() * 1000.0
    }

    /// Number of door preparations that exceeded the declared budget.
    #[func]
    pub fn door_fade_activations(&self) -> i64 {
        i64::try_from(self.door_fade.activations()).unwrap_or(i64::MAX)
    }

    /// Whether the last activate press found something within reach.
    #[func]
    pub fn last_activate_hit(&self) -> bool {
        self.last_activate_hit
    }

    /// Resolved placed reference from the last activate press, or an empty string.
    /// This diagnostic does not imply that default game activation ran.
    #[func]
    pub fn last_activate_reference(&self) -> GString {
        self.last_activate_reference
            .map_or_else(GString::new, |id| GString::from(&id.to_string()))
    }

    /// Start the game VM using the same records and VFS as the renderer.
    #[func]
    pub fn start_gameplay(&mut self) -> bool {
        if std::env::var("SOE_THREADED_SIMULATION").as_deref() == Ok("1") {
            return self.start_threaded_gameplay();
        }
        self.start_game_runtime()
    }

    /// Build navigation caches before a PRF-11 timed gameplay sample.
    #[func]
    pub fn prepare_ai_planning_check(&mut self) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        soe_game::ai::nav_graph(&mut game.world_mut());
        true
    }

    /// Last AI driver tick in microseconds, including door refresh and navigation slices.
    #[func]
    pub fn ai_planning_microseconds(&self) -> f64 {
        self.game.as_ref().map_or(0.0, |game| {
            game.world().ai.planning_time.as_secs_f64() * 1_000_000.0
        })
    }

    /// Inclusive AI phase timings and instantiated actor count for planning diagnostics.
    #[func]
    pub fn ai_planning_details(&self) -> VarDictionary {
        let mut result = VarDictionary::new();
        if let Some(game) = &self.game {
            let world = game.world();
            result.set("door_checks", world.ai.door_checks as i64);
            result.set(
                "door_refresh_us",
                world.ai.door_refresh_time.as_secs_f64() * 1_000_000.0,
            );
            result.set(
                "evaluation_us",
                world.ai.evaluation_time.as_secs_f64() * 1_000_000.0,
            );
            for (phase, duration) in &world.ai.planning_phases {
                result.set(format!("{phase}_us"), duration.as_secs_f64() * 1_000_000.0);
            }
            let actors = world.loaded_actors();
            result.set("instantiated_actors", actors.len() as i64);
            let selected = actors
                .into_iter()
                .filter(|id| {
                    world.actor_ref(*id).is_some_and(|actor| {
                        actor.ai.package.is_some()
                            && !matches!(
                                actor.ai.procedure,
                                soe_game::ai::Procedure::None
                                    | soe_game::ai::Procedure::Unsupported(_)
                            )
                    })
                })
                .count();
            result.set("selected_packages", selected as i64);
            result.set(
                "navmesh_triangles",
                world
                    .ai
                    .nav_graph()
                    .map_or(0, |graph| graph.triangle_count()) as i64,
            );
            for (prefix, workers) in [
                ("navigation_worker", world.ai.planning_workers()),
                ("movement_worker", world.ai.movement_workers()),
            ] {
                result.set(format!("{prefix}_jobs"), workers.jobs as i64);
                result.set(
                    format!("{prefix}_wait_us"),
                    workers.main_thread_wait.as_secs_f64() * 1e6,
                );
                result.set(
                    format!("{prefix}_busy_us"),
                    workers
                        .busy
                        .iter()
                        .map(std::time::Duration::as_secs_f64)
                        .sum::<f64>()
                        * 1e6,
                );
            }
            for (index, name) in ["high", "middle_high", "middle_low", "low"]
                .into_iter()
                .enumerate()
            {
                result.set(
                    format!("{name}_actors"),
                    world.ai.process_counts[index] as i64,
                );
                result.set(
                    format!("{name}_updates"),
                    world.ai.process_updates[index] as i64,
                );
            }
        }
        result
    }

    /// Package evaluations performed by the active gameplay world.
    #[func]
    pub fn ai_package_evaluations(&self) -> i64 {
        self.game
            .as_ref()
            .map_or(0, |game| game.world().ai.evaluations as i64)
    }

    /// Read navigation at supported NPC package positions for the TST-18 frame gate.
    /// Queries read the shared World and do not alter actor or planner state.
    #[func]
    pub fn probe_npc_navigation(&self) -> VarDictionary {
        let _system = soe_trace::SystemScope::new(soe_trace::System::AiPlanning);
        let mut result = vdict! { "queries" => 0_i64, "located" => 0_i64 };
        let Some(game) = &self.game else {
            return result;
        };
        let world = game.world();
        let Some(graph) = world.ai.nav_graph() else {
            return result;
        };
        let mut queries = 0_i64;
        let mut located = 0_i64;
        for actor in world.loaded_actors() {
            if world.is_player(actor)
                || world.actor_ref(actor).is_none_or(|state| {
                    state.ai.package.is_none()
                        || matches!(
                            state.ai.procedure,
                            soe_game::ai::Procedure::None | soe_game::ai::Procedure::Unsupported(_)
                        )
                })
            {
                continue;
            }
            if let Some(reference) = world.reference_ref(actor) {
                queries += 1;
                located += i64::from(graph.locate(reference.position).is_some());
            }
        }
        result.set("queries", queries);
        result.set("located", located);
        result
    }

    /// Apply actor-value damage to the player through the active shared game.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn damage_player_actor_value(&mut self, name: GString, amount: f32) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let player = game.player();
        game.world_mut()
            .damage_actor_value(player, &name.to_string(), amount)
            .is_ok()
    }

    /// Read one reference's current simulated placement in Skyrim units.
    #[func]
    pub fn gameplay_reference_pose(&self, file: i32, object: i64) -> VarDictionary {
        let mut result = VarDictionary::new();
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return result;
        };
        let Some(game) = &self.game else {
            return result;
        };
        let world = game.world();
        let Some(pose) = world.ref_placement(GlobalFormId { file, object }) else {
            return result;
        };
        result.set(
            "position",
            Vector3::new(pose.position[0], pose.position[1], pose.position[2]),
        );
        result.set(
            "rotation",
            Vector3::new(pose.rotation[0], pose.rotation[1], pose.rotation[2]),
        );
        result.set("visible", pose.visible);
        result
    }

    /// Count this reference's movement and Godot query collision bodies.
    #[func]
    pub fn reference_collision_bodies(&self, file: i32, object: i64) -> VarDictionary {
        let mut result = VarDictionary::new();
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return result;
        };
        let reference = GlobalFormId { file, object };
        result.set(
            "movement",
            self.collision_references
                .values()
                .filter(|id| **id == reference)
                .count() as i64,
        );
        result.set(
            "query",
            self.query_collision.active_reference_bodies(reference) as i64,
        );
        result
    }

    /// Set a reference's enabled state through the shared World.
    #[func]
    pub fn set_reference_enabled(&mut self, file: i32, object: i64, enabled: bool) -> bool {
        let (Ok(file), Ok(object), Some(game)) = (
            u16::try_from(file),
            u32::try_from(object),
            self.game.as_mut(),
        ) else {
            return false;
        };
        let mut world = game.world_mut();
        let Some(reference) = world.reference(GlobalFormId { file, object }) else {
            return false;
        };
        if reference.deleted {
            return false;
        }
        world.set_reference_enabled(GlobalFormId { file, object }, enabled);
        true
    }

    /// Set an object's open destination through the shared World, like SetOpen.
    #[func]
    pub fn set_reference_open(&mut self, file: i32, object: i64, opened: bool) -> bool {
        let (Ok(file), Ok(object), Some(game)) = (
            u16::try_from(file),
            u32::try_from(object),
            self.game.as_mut(),
        ) else {
            return false;
        };
        game.world_mut()
            .set_reference_open(GlobalFormId { file, object }, opened)
    }

    /// Remove a reference from its host presentation on the next tick.
    #[func]
    pub fn despawn_reference(&mut self, file: i32, object: i64) -> bool {
        let (Ok(file), Ok(object), Some(game)) = (
            u16::try_from(file),
            u32::try_from(object),
            self.game.as_mut(),
        ) else {
            return false;
        };
        let mut world = game.world_mut();
        let Some(reference) = world.reference(GlobalFormId { file, object }) else {
            return false;
        };
        reference.deleted = true;
        reference.enabled = false;
        true
    }

    /// Queue and immediately capture a screenshot through the host request path.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn take_screenshot(&mut self, path: GString) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        if let Err(error) = game.vm.call_global(
            "Debug",
            "TakeScreenshot",
            &[soe_papyrus::Value::string(&path.to_string())],
        ) {
            soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode: Debug.TakeScreenshot failed: {error}");
            return false;
        }
        game.run_until_idle(64);
        self.capture_screenshot_requests()
    }

    /// Current derived image-space stack for host diagnostics.
    #[func]
    pub fn gameplay_image_space(&self) -> Array<VarDictionary> {
        let mut result = Array::new();
        for layer in self.image_space.layers() {
            let mut row = VarDictionary::new();
            row.set("modifier", &GString::from(&layer.modifier.to_string()));
            row.set("strength", layer.strength);
            row.set("age", layer.age);
            result.push(&row);
        }
        result
    }

    /// Select a WTHR editor id and its zero-based sunrise, day, sunset, night slots.
    /// An empty id restores the loaded cell's XCIM source.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn select_weather_image_space(
        &mut self,
        weather: GString,
        from: i32,
        to: i32,
        blend: f32,
    ) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let selection = if weather.is_empty() {
            None
        } else {
            let Some(id) = game.world().form(&weather.to_string()) else {
                return false;
            };
            let (Ok(from), Ok(to)) = (u8::try_from(from), u8::try_from(to)) else {
                return false;
            };
            Some(soe_game::world::image_space::WeatherSelection {
                weather: id,
                from,
                to,
                blend,
            })
        };
        if let Err(error) = game.world_mut().select_weather_image_space(selection) {
            soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode: {error}");
            return false;
        }
        let result = self.image_space.sync(&game.world());
        self.refresh_image_space();
        if let Err(error) = result {
            soe_trace::error!(target: "host::player", system = "host::player", "PlayerNode: {error}");
            return false;
        }
        true
    }

    /// Current EffectShader applications for host presentation diagnostics.
    #[func]
    pub fn gameplay_effect_shaders(&self) -> Array<VarDictionary> {
        let mut result = Array::new();
        for playback in self.effect_shaders.playbacks() {
            let mut row = VarDictionary::new();
            row.set("shader", &GString::from(&playback.shader.to_string()));
            row.set("target", &GString::from(&playback.target.to_string()));
            row.set("infinite", playback.remaining.is_none());
            if let Some(remaining) = playback.remaining {
                row.set("remaining", remaining);
            }
            result.push(&row);
        }
        result
    }

    /// Winning RFCT records with a resolved art object or effect shader.
    #[func]
    pub fn gameplay_visual_effect_forms(&self) -> Array<VarDictionary> {
        let mut result = Array::new();
        let Some(game) = &self.game else {
            return result;
        };
        for (id, _) in game.world().order().winners_of(soe_esm::Tag::new(b"RFCT")) {
            let Some(data) = game.world().visual_effect_data(id) else {
                continue;
            };
            if data.art.is_none() && data.shader.is_none() {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set("file", i32::from(id.file));
            row.set("object", i64::from(id.object));
            row.set("art", data.art.is_some());
            row.set("shader", data.shader.is_some());
            row.set("flags", i64::from(data.flags));
            result.push(&row);
        }
        result
    }

    /// Start an RFCT effect through the shared game's World API.
    #[func]
    pub fn gameplay_play_visual_effect(
        &mut self,
        effect_file: i32,
        effect_object: i64,
        target_file: i32,
        target_object: i64,
        duration: f64,
    ) -> bool {
        let (Ok(effect_file), Ok(effect_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(effect_file),
            u32::try_from(effect_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return false;
        };
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        game.world_mut()
            .play_visual_effect(
                GlobalFormId {
                    file: effect_file,
                    object: effect_object,
                },
                GlobalFormId {
                    file: target_file,
                    object: target_object,
                },
                duration,
                None,
            )
            .is_ok()
    }

    /// Stop RFCT instances for the exact effect and target pair.
    #[func]
    pub fn gameplay_stop_visual_effect(
        &mut self,
        effect_file: i32,
        effect_object: i64,
        target_file: i32,
        target_object: i64,
    ) -> i64 {
        let (Ok(effect_file), Ok(effect_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(effect_file),
            u32::try_from(effect_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return -1;
        };
        let Some(game) = self.game.as_mut() else {
            return -1;
        };
        game.world_mut()
            .stop_visual_effect(
                GlobalFormId {
                    file: effect_file,
                    object: effect_object,
                },
                GlobalFormId {
                    file: target_file,
                    object: target_object,
                },
            )
            .map_or(-1, |count| i64::try_from(count).unwrap_or(i64::MAX))
    }

    /// Logical cell currently represented by the host.
    #[func]
    pub fn gameplay_cell(&self) -> GString {
        self.loaded_cell
            .map_or_else(GString::new, |id| GString::from(&id.to_string()))
    }

    /// Animation graph events that reached the active Game, for host diagnostics.
    #[func]
    pub fn gameplay_animation_events(&self) -> Array<VarDictionary> {
        let mut result = Array::new();
        if let Some(game) = &self.game {
            for (source, name) in &game.world().animation_events {
                let mut row = VarDictionary::new();
                row.set("source_file", i64::from(source.file));
                row.set("source_object", i64::from(source.object));
                row.set("name", &GString::from(name));
                result.push(&row);
            }
        }
        result
    }

    /// Resident exterior cells and their physics handles for integration diagnostics.
    #[func]
    pub fn exterior_resources(&self) -> VarDictionary {
        let mut result = VarDictionary::new();
        if let Some(exterior) = &self.exterior {
            for (cell, bodies) in &exterior.cells {
                let ids: PackedInt64Array = bodies.iter().map(|body| i64::from(body.0)).collect();
                result.set(cell.to_string(), &ids);
            }
        }
        result
    }

    /// CPU prefetch state for the installed host acceptance harness.
    #[func]
    pub fn interior_prefetch_status(&mut self) -> VarDictionary {
        self.interior_prefetch.poll();
        let mut row = VarDictionary::new();
        row.set("started", self.interior_prefetch.started as i64);
        row.set("hits", self.interior_prefetch.hits as i64);
        row.set("rejected", self.interior_prefetch.rejected as i64);
        row.set("pending", self.interior_prefetch.pending());
        row.set("payload_bytes", self.interior_prefetch.payload_bytes as i64);
        row.set(
            "ready",
            self.interior_prefetch
                .ready_cell()
                .map_or_else(String::new, |cell| cell.to_string()),
        );
        row
    }

    /// Door links in the current logical cell for integration tools.
    #[func]
    pub fn gameplay_doors(&mut self) -> Array<VarDictionary> {
        let (Some(game), Some(cell)) = (&mut self.game, self.loaded_cell) else {
            return Array::new();
        };
        let mut world = game.world_mut();
        let doors = world.cell_doors(cell);
        let mut result = Array::new();
        for (id, far) in doors {
            let Some(far) = far else {
                continue;
            };
            let mut row = VarDictionary::new();
            row.set("file", i64::from(id.file));
            row.set("object", i64::from(id.object));
            row.set("far_file", i64::from(far.file));
            row.set("far_object", i64::from(far.object));
            if let Some(name) = world
                .reference(far)
                .and_then(|reference| reference.cell)
                .and_then(|cell| self.data_source.as_ref()?.exterior_world(cell))
                .and_then(|worldspace| world.record(worldspace)?.editor_id())
            {
                row.set("far_worldspace", &GString::from(&name));
            }
            row.set("locked", world.reference(id).is_none_or(|r| r.locked));
            if let Some((return_door, landing)) = world.reference(far).and_then(|r| r.teleport) {
                if return_door == id {
                    row.set(
                        "return_position",
                        Vector3::new(
                            landing.position[0],
                            landing.position[1],
                            landing.position[2],
                        ),
                    );
                    row.set("return_yaw", landing.rotation[2]);
                }
            }
            result.push(&row);
        }
        result
    }

    /// Creation benches in the current logical cell, for gameplay tools.
    #[func]
    pub fn gameplay_creation_benches(&mut self) -> Array<VarDictionary> {
        let (Some(game), Some(cell)) = (&mut self.game, self.loaded_cell) else {
            return Array::new();
        };
        let mut references = game.world_mut().cell_references(cell);
        references.sort_unstable();
        let mut result = Array::new();
        for id in references {
            if !crate::crafting_menu::is_creation_bench(game, id) {
                continue;
            }
            let active = game
                .world_mut()
                .reference(id)
                .is_some_and(|reference| reference.enabled && !reference.deleted);
            if !active {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set("file", i64::from(id.file));
            row.set("object", i64::from(id.object));
            result.push(&row);
        }
        result
    }

    /// Unlocked non-teleport doors in the current cell, for gameplay tools.
    #[func]
    pub fn gameplay_swing_doors(&self) -> Array<VarDictionary> {
        let (Some(game), Some(cell), Some(source)) =
            (&self.game, self.loaded_cell, &self.data_source)
        else {
            return Array::new();
        };
        let world = game.world();
        let mut result = Array::new();
        for id in source.cell_index().references(cell).iter().copied() {
            let Some(reference_record) = source.order().winner(id) else {
                continue;
            };
            if reference_record.find(soe_esm::Tag::new(b"XTEL")).is_some() {
                continue;
            }
            let Some(record_ref) = source.order().winner_ref(id) else {
                continue;
            };
            let Ok(reference) = soe_esm::records::refr::Refr::from_record(reference_record) else {
                continue;
            };
            let base = source.order().resolve(record_ref.file, reference.base);
            if source
                .order()
                .winner(base)
                .is_none_or(|record| record.tag != soe_esm::Tag::new(b"DOOR"))
                || world.reference_ref(id).is_some_and(|state| state.locked)
            {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set("file", i64::from(id.file));
            row.set("object", i64::from(id.object));
            result.push(&row);
        }
        result
    }

    /// Number of door control nodes attached to the gameplay streamer.
    #[func]
    pub fn swing_door_count(&self) -> i32 {
        self.game_streamer
            .as_ref()
            .map_or(0, |streamer| streamer.bind().door_count())
    }

    /// Drive the same activation path from a runtime harness using a resolved FormID.
    #[func]
    pub fn activate_reference(&mut self, file: i32, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let ok = self.activate_game_reference(GlobalFormId { file, object });
        self.sync_game_pose() && ok
    }

    /// Return the pending lockpick target and authored lock level for the UI.
    #[func]
    pub fn pending_lockpick(&self) -> VarDictionary {
        let mut result = VarDictionary::new();
        let Some(target) = self.pending_lockpick else {
            return result;
        };
        let Some(game) = &self.game else {
            return result;
        };
        let world = game.world();
        let Some(reference) = world.reference_ref(target) else {
            return result;
        };
        result.set("file", i64::from(target.file));
        result.set("object", i64::from(target.object));
        result.set("level", i64::from(reference.lock_level));
        result
    }

    /// Resolve the pending lockpick interaction and continue normal activation
    /// after a successful pick.
    #[func]
    pub fn resolve_pending_lockpick(&mut self, success: bool) -> bool {
        let Some(target) = self.pending_lockpick.take() else {
            return false;
        };
        let Some(game) = &mut self.game else {
            return false;
        };
        let player = game.player();
        let accepted = game
            .world()
            .reference_ref(target)
            .is_some_and(|reference| reference.locked && reference.lock_level != 255)
            && game
                .world()
                .form("Lockpick")
                .is_some_and(|pick| game.world_mut().item_count(player, pick) > 0);
        if !accepted {
            return false;
        }
        let unlocked = game.resolve_lockpick(target, player, success);
        if unlocked {
            game.activate_deferred(target, player)
        } else {
            true
        }
    }

    /// Point the camera, used when a door or a save sets a facing.
    #[func]
    pub fn set_facing(&mut self, yaw: f32, pitch: f32) {
        self.rig.set_facing(yaw, pitch);
        self.update_camera();
    }

    /// Read the camera controls currently applied from Game natives.
    #[func]
    pub fn camera_state(&self) -> VarDictionary {
        let mut state = VarDictionary::new();
        let target = self.camera_state.target();
        state.set("target_file", target.map_or(-1, |id| i64::from(id.file)));
        state.set(
            "target_object",
            target.map_or(-1, |id| i64::from(id.object)),
        );
        state.set(
            "first_person_geometry",
            self.camera_state.first_person_geometry_visible(),
        );
        state.set(
            "sitting_rotation_degrees",
            self.camera_state.sitting_rotation_degrees(),
        );
        state
    }

    /// The action names this node registered, for a test or a rebinding UI.
    #[func]
    pub fn action_names(&self) -> PackedStringArray {
        Action::all()
            .iter()
            .map(|a| GString::from(a.event_name()))
            .collect()
    }
}

fn player_locomotion(intent: crate::input::Intent, paralyzed: bool) -> (Vec3, f32, bool) {
    if paralyzed {
        (Vec3::ZERO, 0.0, false)
    } else {
        (intent.direction, intent.speed, intent.jump)
    }
}

impl PlayerNode {
    fn look_stick(&mut self, delta: f64) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Input);
        let (x, y) = self.devices.look(&self.bindings);
        let scale = self.gamepad_look_speed * delta as f32;
        self.rig.look(x * scale, y * scale);
    }

    fn character(&self) -> Option<&soe_physics::Character> {
        let world = self.world.as_ref()?;
        world.character(self.character?)
    }

    fn feet(&self) -> Vec3 {
        self.character()
            .map_or(Vec3::new(0.0, 0.0, 0.0), |c| c.position)
    }

    fn capsule_height(&self) -> f32 {
        self.character()
            .map_or(CharacterSettings::default().height, |c| c.height())
    }

    /// Read the PC control map through the archive and loose-file overlay.
    /// A missing map uses fallback bindings and reports the missing input.
    fn load_control_map(&mut self, source: &DataSource) {
        let mut bindings = Bindings::default();
        match source.vfs().read("interface/controls/pc/controlmap.txt") {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                bindings.apply_control_map(&ControlMap::parse(&text));
            }
            Err(e) => {
                soe_trace::warn!(target: "host::player", system = "host::player", "PlayerNode: no control map read ({e}), using the defaults");
            }
        }
        self.bindings = bindings;
    }

    /// Publish every action into Godot's `InputMap` under its vanilla event
    /// name.
    ///
    /// A chord of several keys cannot be one `InputEventKey`, so a multi-key
    /// binding registers only its last key here and the node's own polling
    /// path is what honours the full chord. None of the default bindings is a
    /// chord, so this affects only a control map that introduces one.
    fn register_actions(&mut self) {
        self.devices.rebind(&self.bindings);
        let mut input = Input::singleton();
        self.held_actions = Action::all()
            .iter()
            .copied()
            .filter(|a| {
                let name = StringName::from(a.event_name());
                InputMap::singleton().has_action(&name)
                    && (input.is_action_pressed(&name) || input.is_action_just_pressed(&name))
            })
            .collect();
        self.held_actions
            .extend(Action::all().into_iter().filter(|a| self.devices.held(*a)));
        let until = Engine::singleton().get_process_frames() + 1;
        self.action_latches
            .extend(self.held_actions.iter().map(|a| (*a, until)));
        let mut map = InputMap::singleton();
        for action in Action::all() {
            let name = StringName::from(action.event_name());
            if map.has_action(&name) {
                input.action_release(&name);
                map.erase_action(&name);
            }
            map.add_action(&name);
            let Some(binding) = self.bindings.get(action) else {
                continue;
            };
            for chord in &binding.0 {
                let Some(last) = chord.0.last() else { continue };
                let Some(code) = godot_key_for(*last) else {
                    soe_trace::warn!(target: "host::player", system = "host::player",
                        "PlayerNode: {} binds {last:#x}, which has no Godot key",
                        action.event_name()
                    );
                    continue;
                };
                let mut event = InputEventKey::new_gd();
                event.set_keycode(Key::from_ord(code as i32));
                map.action_add_event(&name, &event);
            }
            if let Some(binding) = self.bindings.mouse(action) {
                for chord in &binding.0 {
                    if let Some(index) = chord.0.last().and_then(|code| godot_mouse_for(*code)) {
                        let mut event = InputEventMouseButton::new_gd();
                        event.set_button_index(MouseButton::from_ord(index));
                        map.action_add_event(&name, &event);
                    }
                }
            }
            if let Some(binding) = self.bindings.gamepad(action) {
                for chord in &binding.0 {
                    match chord.0.last().and_then(|code| godot_pad_for(*code)) {
                        Some(PadControl::Button(index)) => {
                            let mut event = InputEventJoypadButton::new_gd();
                            event.set_device(-1);
                            event.set_button_index(JoyButton::from_ord(index));
                            map.action_add_event(&name, &event);
                        }
                        Some(PadControl::Trigger(index)) => {
                            let mut event = InputEventJoypadMotion::new_gd();
                            event.set_device(-1);
                            event.set_axis(JoyAxis::from_ord(index));
                            event.set_axis_value(1.0);
                            map.action_add_event(&name, &event);
                        }
                        None => {}
                    }
                }
            }
        }
    }

    /// Read full chords from device state and consume each press once.
    fn gather(&mut self) -> InputState {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Input);
        if !self.input_focused {
            return InputState::default();
        }
        let input = Input::singleton();
        let held_now: BTreeSet<Action> = Action::all()
            .into_iter()
            .filter(|&a| {
                self.devices.held(a)
                    || if self.bindings.uses_action_state(a) {
                        input.is_action_pressed(&StringName::from(a.event_name()))
                    } else {
                        false
                    }
            })
            .collect();
        let frame = Engine::singleton().get_process_frames();
        self.action_latches.retain(|_, until| frame <= *until);
        let mut pressed_now = self.devices.take_pressed();
        for action in Action::all() {
            if self.bindings.uses_action_state(action) {
                let name = StringName::from(action.event_name());
                let rising = held_now.contains(&action) && !self.held_actions.contains(&action);
                let short_tap = input.is_action_just_pressed(&name)
                    && !self.action_latches.contains_key(&action);
                if rising || short_tap {
                    pressed_now.insert(action);
                }
            }
        }
        self.action_latches.extend(
            pressed_now
                .iter()
                .filter(|a| self.bindings.uses_action_state(**a))
                .map(|a| (*a, frame + 1)),
        );
        let held = |a: Action| held_now.contains(&a);
        let pressed = |a: Action| pressed_now.contains(&a);
        let axis = |neg: Action, pos: Action| {
            f32::from(u8::from(held(pos))) - f32::from(u8::from(held(neg)))
        };
        let stick = self.devices.movement(&self.bindings);
        let state = InputState {
            move_x: axis(Action::StrafeLeft, Action::StrafeRight) + stick.0,
            move_y: axis(Action::Back, Action::Forward) + stick.1,
            look_dx: 0.0,
            look_dy: 0.0,
            sprint: held(Action::Sprint),
            walk: held(Action::Walk),
            sneak_held: held(Action::Sneak),
            jump_pressed: pressed(Action::Jump),
            sneak_pressed: pressed(Action::Sneak),
            activate_pressed: pressed(Action::Activate),
            toggle_pov_pressed: pressed(Action::TogglePov),
            toggle_always_run_pressed: pressed(Action::ToggleAlwaysRun),
            right_attack_pressed: pressed(Action::RightAttack),
            left_attack_held: held(Action::LeftAttackBlock),
            right_attack_held: held(Action::RightAttackBlock),
            left_attack_pressed: pressed(Action::LeftAttackBlock),
        };
        self.held_actions = held_now;
        state
    }

    fn create_door_fade(&mut self) {
        let mut canvas = CanvasLayer::new_alloc();
        canvas.set_name("DoorFadeLayer");
        canvas.set_layer(90);
        let mut overlay = ColorRect::new_alloc();
        overlay.set_name("DoorFade");
        overlay.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        overlay.set_mouse_filter(MouseFilter::IGNORE);
        overlay.hide();
        canvas.add_child(&overlay);
        self.base_mut().add_child(&canvas);
        overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.door_fade_view = Some(DoorFadeView { overlay });
    }

    fn create_image_space_tint(&mut self) {
        let mut canvas = CanvasLayer::new_alloc();
        canvas.set_name("ImageSpaceTintLayer");
        canvas.set_layer(1);
        let mut overlay = ColorRect::new_alloc();
        overlay.set_name("ImageSpaceTint");
        overlay.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        overlay.set_mouse_filter(MouseFilter::IGNORE);
        overlay.hide();
        canvas.add_child(&overlay);
        self.image_space_modifier_renderer.attach(&mut canvas);
        self.base_mut().add_child(&canvas);
        overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.image_space_tint = Some(ImageSpaceTintView { overlay });
    }

    fn refresh_image_space(&mut self) {
        let rendered = self
            .image_space
            .base()
            .map(crate::image_space::RenderedBase::from);
        let modifiers = self
            .game
            .as_ref()
            .and_then(|game| {
                self.image_space
                    .render_layers(&game.world())
                    .ok()
                    .map(|layers| crate::image_space::RenderedModifiers::from_layers(&layers))
            })
            .unwrap_or_default();
        if let Some(camera) = &mut self.camera {
            self.image_space_renderer
                .apply(camera, rendered.as_ref(), &modifiers);
        }
        self.image_space_modifier_renderer.apply(&modifiers);
        if let Some(tint) = &mut self.image_space_tint {
            if let Some(values) = rendered.filter(|values| values.tint[0] > 0.0) {
                tint.overlay.set_color(Color::from_rgba(
                    values.tint[1].clamp(0.0, 1.0),
                    values.tint[2].clamp(0.0, 1.0),
                    values.tint[3].clamp(0.0, 1.0),
                    values.tint[0].clamp(0.0, 1.0),
                ));
                tint.overlay.show();
            } else {
                tint.overlay.hide();
            }
        }
    }

    pub(super) fn record_door_texture_preparation(&mut self, elapsed: Duration) {
        self.door_fade.record_preparation(elapsed);
        self.refresh_door_fade();
    }

    fn advance_door_fade(&mut self, delta: f64) {
        if delta.is_finite() && delta > 0.0 {
            self.door_fade.advance(Duration::from_secs_f64(delta));
            self.game_fade.advance(delta as f32);
            self.refresh_door_fade();
        }
    }

    fn refresh_door_fade(&mut self) {
        let presentation =
            crate::door_fade::fade_overlay_presentation(&self.door_fade, &self.game_fade);
        if let Some(view) = &mut self.door_fade_view {
            let [red, green, blue, alpha] = presentation.rgba;
            view.overlay
                .set_color(Color::from_rgba(red, green, blue, alpha));
            view.overlay.set_visible(presentation.visible);
        }
    }

    fn create_camera(&mut self) {
        let mut camera = Camera3D::new_alloc();
        camera.set_near(0.05);
        camera.set_far(4096.0);
        self.base_mut().add_child(&camera);
        camera.make_current();
        self.camera = Some(camera);
        self.update_camera();
    }

    /// Place the camera where the rig says, converting into Godot's space.
    fn update_camera(&mut self) {
        self.place_camera(self.feet(), self.capsule_height());
    }

    fn update_camera_shake(&mut self, delta: f32) {
        let offsets = if let Some((elapsed, duration, strength)) = self.camera_shake {
            let next = elapsed + delta.max(0.0);
            if next >= duration {
                self.camera_shake = None;
                self.camera_shake_time = 0.0;
                (0.0, 0.0)
            } else {
                self.camera_shake = Some((next, duration, strength));
                self.camera_shake_time += delta.max(0.0);
                let fade = (1.0 - next / duration.max(f32::EPSILON)).clamp(0.0, 1.0);
                let phase = self.camera_shake_time * 38.0;
                (
                    strength * fade * 0.012 * phase.sin(),
                    strength * fade * 0.012 * (phase * 1.37).cos(),
                )
            }
        } else {
            (0.0, 0.0)
        };
        if let Some(camera) = &mut self.camera {
            camera.set_h_offset(offsets.0);
            camera.set_v_offset(offsets.1);
        }
    }

    fn place_camera(&mut self, feet: Vec3, height: f32) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Render);
        let _phase = crate::scene_timing::Scope::new(soe_trace::System::Input);
        if self.camera.is_none() {
            return;
        }
        let (feet, height) = self.camera_anchor(feet, height);

        // In third person the boom is pulled in by whatever it would pass
        // through, so the camera never ends up inside a wall.
        let clear = if self.rig.mode == CameraMode::Third {
            let head = self.rig.head(feet, height);
            let target = self.rig.boom_target(feet, height);
            self.world.as_ref().and_then(|w| {
                w.raycast(head, target).map(|hit| {
                    let d = Vec3::new(
                        hit.point.x - head.x,
                        hit.point.y - head.y,
                        hit.point.z - head.z,
                    );
                    d.length()
                })
            })
        } else {
            None
        };

        let eye = self.rig.eye(feet, height, clear);
        let transform = godot_transform(&eye);
        if let Some(camera) = &mut self.camera {
            camera.set_global_transform(transform);
            camera.set_fov(self.rig.fov());
        }
    }

    fn camera_anchor(&mut self, player_feet: Vec3, player_height: f32) -> (Vec3, f32) {
        let Some(target) = self.camera_state.target() else {
            return (player_feet, player_height);
        };
        let Some(game) = &self.game else {
            return (player_feet, player_height);
        };
        let mut world = game.world_mut();
        let target_pose = world.reference(target).map(|reference| {
            (
                target,
                Vec3::new(
                    reference.position[0],
                    reference.position[1],
                    reference.position[2],
                ),
                CharacterSettings::default().height,
            )
        });
        self.camera_state
            .anchor(player_feet, player_height, target_pose)
    }
}

/// An [`Eye`] into a Godot camera transform.
///
/// The rotation comes from `Eye::basis_columns`, which is plain arithmetic in
/// `camera` with tests behind it, rather than from Euler angles here. Skyrim's
/// yaw and Godot's rotation order disagree in ways that compile either way and
/// render wrong, so the convention is settled where it can be checked.
fn godot_transform(eye: &Eye) -> Transform3D {
    let [x, y, z] = eye.basis_columns();
    let col = |v: [f32; 3]| Vector3::new(v[0], v[1], v[2]);
    let origin = eye.position.to_godot();
    Transform3D::new(
        Basis::from_cols(col(x), col(y), col(z)),
        Vector3::new(origin.x, origin.y, origin.z),
    )
}

#[cfg(test)]
mod mag_42_tests {
    use super::player_locomotion;
    use crate::input::Intent;
    use soe_physics::Vec3;

    #[test]
    fn mag_42_paralysis_blocks_player_locomotion_and_jump() {
        let intent = Intent {
            direction: Vec3::new(1.0, 0.0, 0.0),
            speed: 120.0,
            jump: true,
            crouch: false,
            activate: false,
            sprinting: false,
        };
        assert_eq!(player_locomotion(intent, true), (Vec3::ZERO, 0.0, false));
    }

    #[test]
    fn mag_42_player_locomotion_returns_when_not_paralyzed() {
        let intent = Intent {
            direction: Vec3::new(1.0, 0.0, 0.0),
            speed: 120.0,
            jump: true,
            crouch: false,
            activate: false,
            sprinting: false,
        };
        assert_eq!(
            player_locomotion(intent, false),
            (intent.direction, intent.speed, intent.jump)
        );
    }
}
