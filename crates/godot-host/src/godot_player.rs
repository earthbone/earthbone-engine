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

use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{
    Camera3D, CanvasLayer, ColorRect, Engine, INode3D, Input, InputEventJoypadButton,
    InputEventJoypadMotion, InputEventKey, InputEventMouseButton, InputEventMouseMotion, InputMap,
    Node3D,
};
use godot::global::{JoyAxis, JoyButton, Key, MouseButton};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_physics::{BodyId, CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};

use crate::camera::{CameraMode, CameraMotion, CameraRig, Eye};
use crate::data_source::DataSource;
use crate::door_fade::DoorFade;
use crate::input::devices::{godot_mouse_for, godot_pad_for, Devices, PadControl};
use crate::input::{godot_key_for, Action, Bindings, ControlMap, Driver, InputState};

mod containers;
mod exterior;
mod footsteps;
mod gameplay;
mod magic;
mod melee;
mod menu;
mod saves;

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
    rig: CameraRig,
    motion: CameraMotion,
    bindings: Bindings,
    driver: Driver,
    world: Option<SimpleWorld>,
    query_collision: crate::godot_physics::CellBodies,
    collision_references: HashMap<BodyId, GlobalFormId>,
    character: Option<BodyId>,
    data_source: Option<Arc<DataSource>>,
    game: Option<soe_game::Game>,
    melee_contact: crate::combat::MeleeContact,
    melee_bodies: HashMap<Rid, GlobalFormId>,
    /// One server body per loaded actor, rebuilt from the shared World on
    /// each physics tick. No scene node is allocated per reference.
    actor_bodies: crate::godot_physics::ActorBodies,
    /// The form the last HitFrame contact query struck, for diagnostics.
    last_melee_target: Option<GlobalFormId>,
    image_space: crate::image_space::ImageSpaceStack,
    image_space_renderer: crate::image_space::renderer::BaseRenderer,
    image_space_tint: Option<ImageSpaceTintView>,
    sound_bank: Option<Gd<crate::godot_audio::SoundBank>>,
    footstep_cadence: crate::audio::FootstepCadence,
    footstep_count: u64,
    footstep_errors: u64,
    game_streamer: Option<Gd<crate::godot_layer::CellStreamerNode>>,
    loaded_cell: Option<GlobalFormId>,
    door_fade: DoorFade,
    door_fade_view: Option<DoorFadeView>,
    exterior: Option<exterior::ExteriorRuntime>,
    interior_prefetch: crate::interior_prefetch::InteriorPrefetch,
    interior_prefetch_elapsed: f64,
    camera: Option<Gd<Camera3D>>,
    /// Mouse motion accumulated since the last camera or physics update, so a frame that
    /// delivers two motion events turns twice as far rather than once.
    pending_look: (f32, f32),
    /// Whether the activate ray hit something on the last step, reported to
    /// GDScript so a prompt can be drawn without a second raycast.
    last_activate_hit: bool,
    last_activate_reference: Option<GlobalFormId>,
    action_latches: BTreeMap<Action, u64>,
    held_actions: BTreeSet<Action>,
    devices: Devices,
    input_focused: bool,
    menu: crate::ui::MenuState,
    inventory_category: crate::inventory_menu::Category,
    magic_hand: soe_game::combat::Hand,
    menu_view: Option<menu::MenuView>,
    container_menu: Option<(GlobalFormId, crate::inventory_menu::Category)>,
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
            rig: CameraRig::default(),
            motion: CameraMotion::new(Vec3::ZERO, CharacterSettings::default().height),
            bindings: Bindings::default(),
            driver: Driver::new(),
            world: None,
            query_collision: crate::godot_physics::CellBodies::default(),
            collision_references: HashMap::new(),
            character: None,
            data_source: None,
            game: None,
            melee_contact: crate::combat::MeleeContact::default(),
            melee_bodies: HashMap::new(),
            actor_bodies: crate::godot_physics::ActorBodies::default(),
            last_melee_target: None,
            image_space: crate::image_space::ImageSpaceStack::default(),
            image_space_renderer: crate::image_space::renderer::BaseRenderer::default(),
            image_space_tint: None,
            sound_bank: None,
            footstep_cadence: crate::audio::FootstepCadence::default(),
            footstep_count: 0,
            footstep_errors: 0,
            game_streamer: None,
            loaded_cell: None,
            door_fade: DoorFade::default(),
            door_fade_view: None,
            exterior: None,
            interior_prefetch: crate::interior_prefetch::InteriorPrefetch::default(),
            interior_prefetch_elapsed: 0.0,
            camera: None,
            pending_look: (0.0, 0.0),
            last_activate_hit: false,
            last_activate_reference: None,
            action_latches: BTreeMap::new(),
            held_actions: BTreeSet::new(),
            devices: Devices::default(),
            input_focused: true,
            menu: crate::ui::MenuState::default(),
            inventory_category: crate::inventory_menu::Category::All,
            magic_hand: soe_game::combat::Hand::Right,
            menu_view: None,
            container_menu: None,
            restore_mouse_on_focus: None,
        }
    }

    fn ready(&mut self) {
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

        self.create_menu();
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::CAPTURED);
        }
    }

    fn unhandled_input(&mut self, event: Gd<godot::classes::InputEvent>) {
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
        if !self.input_focused {
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
                return;
            }
        }
        if self.menu.is_open() {
            return;
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
        if self.auto_step {
            self.advance(delta);
            self.sync_actor_bodies();
        }
    }

    fn process(&mut self, delta: f64) {
        if self.auto_step && !self.menu.is_open() {
            self.look_stick(delta);
            let (dx, dy) = std::mem::take(&mut self.pending_look);
            self.rig.look(dx, dy);
            let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
            let (feet, height) = self.motion.sample(fraction);
            self.place_camera(feet, height);
        }
    }

    fn exit_tree(&mut self) {
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        self.query_collision.free();
        self.actor_bodies.free();

        self.close_menu();
        self.menu_view = None;
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::VISIBLE);
        }
        self.camera = None;
    }
}

#[godot_api]
impl PlayerNode {
    /// Emitted after opening through gdext's reentrant base guard.
    #[signal]
    fn menu_opened(title: GString);

    /// Emitted after closing and releasing owned content controls.
    #[signal]
    fn menu_closed();

    #[func]
    fn input_focus_lost(&mut self) {
        self.input_focused = false;
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

    /// Connect an already loaded character rig to this gameplay world's live
    /// skin and equipment records. Reuses the existing character node.
    #[func]
    pub fn bind_actor_appearance(
        &mut self,
        mut visual: Gd<crate::godot_actor::SkinnedActorNode>,
        file: i32,
        object: i64,
    ) -> bool {
        let (Some(game), Some(source)) = (&self.game, &self.data_source) else {
            return false;
        };
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        match visual.bind_mut().bind_appearance(
            game.shared_world(),
            Arc::clone(source),
            soe_esm::GlobalFormId { file, object },
        ) {
            Ok(()) => true,
            Err(error) => {
                godot_error!("PlayerNode actor appearance: {error}");
                false
            }
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
        self.interior_prefetch = crate::interior_prefetch::InteriorPrefetch::default();
        match DataSource::open(Path::new(&path.to_string())) {
            Ok(source) => {
                self.load_control_map(&source);
                if self.base().is_node_ready() {
                    self.register_actions();
                }
                self.data_source = Some(Arc::new(source));
                self.game = None;
                self.image_space = crate::image_space::ImageSpaceStack::default();
                self.refresh_image_space();
                self.exterior = None;
                self.game_streamer = None;
                true
            }
            Err(e) => {
                godot_error!("PlayerNode::load_data: {e}");
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
        self.data_source = Some(source);
        self.game = None;
        self.image_space = crate::image_space::ImageSpaceStack::default();
        self.refresh_image_space();
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
            godot_error!("PlayerNode::enter_cell: load_data was not called");
            return -1;
        };
        let Some(cell_id) = source.resolve_cell(&editor_id.to_string()) else {
            godot_error!("PlayerNode::enter_cell: no cell named {editor_id}");
            return -1;
        };
        let collision = match source.collide_cell_id(cell_id) {
            Ok(c) => c,
            Err(e) => {
                godot_error!("PlayerNode::enter_cell: {e}");
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
        self.refresh_door_fade();
        self.interior_prefetch.invalidate();
        self.melee_contact.cancel();
        self.melee_bodies.clear();
        self.exterior = None;
        self.collision_references = collision.references;
        self.last_activate_hit = false;
        self.last_activate_reference = None;
        self.motion = CameraMotion::new(self.feet(), self.capsule_height());
        self.update_camera();
        bodies as i32
    }

    /// One simulation step. Public so a scripted run can drive a fixed step
    /// instead of relying on frame timing.
    #[func]
    pub fn advance(&mut self, delta: f64) {
        if self.menu.is_open() {
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

        let input = self.gather();
        if input.toggle_pov_pressed {
            self.rig.mode = self.rig.mode.toggled();
        }

        let camera = self.rig;
        let intent = self.driver.update(&input, &camera);
        let before = self.feet();
        let was_grounded = self.character().is_some_and(|c| c.on_ground);

        if let (Some(world), Some(id)) = (self.world.as_mut(), self.character) {
            world.set_crouch(id, intent.crouch);
            world.walk(id, intent.direction, intent.speed);
            if intent.jump {
                world.try_jump(id);
            }
            world.step(delta as f32);
        }

        self.advance_footsteps(before, was_grounded, &input);

        let activation_hit = if input.activate_pressed {
            self.activate_ray()
        } else {
            None
        };
        self.last_activate_hit = activation_hit.is_some();
        self.last_activate_reference =
            activation_hit.and_then(|hit| self.collision_references.get(&hit.body).copied());

        self.advance_game(delta);
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
        self.start_game_runtime()
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
            godot_error!("PlayerNode: {error}");
            return false;
        }
        let result = self.image_space.sync(&game.world());
        self.refresh_image_space();
        if let Err(error) = result {
            godot_error!("PlayerNode: {error}");
            return false;
        }
        true
    }

    /// Logical cell currently represented by the host.
    #[func]
    pub fn gameplay_cell(&self) -> GString {
        self.loaded_cell
            .map_or_else(GString::new, |id| GString::from(&id.to_string()))
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
            row.set("locked", world.reference(id).is_none_or(|r| r.locked));
            result.push(&row);
        }
        result
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

    /// Point the camera, used when a door or a save sets a facing.
    #[func]
    pub fn set_facing(&mut self, yaw: f32, pitch: f32) {
        self.rig.set_facing(yaw, pitch);
        self.update_camera();
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

impl PlayerNode {
    fn look_stick(&mut self, delta: f64) {
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
                godot_warn!("PlayerNode: no control map read ({e}), using the defaults");
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
                    godot_warn!(
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
        };
        self.held_actions = held_now;
        state
    }

    /// Cast from the eye along the look direction, the way activating works.
    fn activate_ray(&self) -> Option<soe_physics::world::RayHit> {
        let world = self.world.as_ref()?;
        let head = self.rig.head(self.feet(), self.capsule_height());
        let dir = self.rig.look_direction();
        let to = Vec3::new(
            head.x + dir.x * ACTIVATE_REACH,
            head.y + dir.y * ACTIVATE_REACH,
            head.z + dir.z * ACTIVATE_REACH,
        );
        world.raycast(head, to)
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
        self.base_mut().add_child(&canvas);
        overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.image_space_tint = Some(ImageSpaceTintView { overlay });
    }

    fn refresh_image_space(&mut self) {
        let rendered = self
            .image_space
            .base()
            .map(crate::image_space::RenderedBase::from);
        if let Some(camera) = &mut self.camera {
            self.image_space_renderer.apply(camera, rendered.as_ref());
        }
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
            self.refresh_door_fade();
        }
    }

    fn refresh_door_fade(&mut self) {
        let opacity = self.door_fade.opacity();
        if let Some(view) = &mut self.door_fade_view {
            view.overlay
                .set_color(Color::from_rgba(0.0, 0.0, 0.0, opacity));
            view.overlay.set_visible(opacity > 0.0);
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

    fn place_camera(&mut self, feet: Vec3, height: f32) {
        if self.camera.is_none() {
            return;
        }

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
