// SPDX-License-Identifier: GPL-3.0-or-later

//! The node that turns a keyboard and a mouse into a character walking
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
//! rendered one cannot drift, and the camera is a `RenderingServer` camera
//! rather than a `Camera3D` node for the reason the rest of the crate avoids
//! nodes.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use godot::classes::{
    Engine, INode3D, Input, InputEventKey, InputEventMouseMotion, InputMap, Node3D,
    RenderingServer, Viewport,
};
use godot::global::Key;
use godot::prelude::*;
use soe_physics::{BodyId, CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};

use crate::camera::{CameraMode, CameraMotion, CameraRig, Eye};
use crate::data_source::DataSource;
use crate::input::{godot_key_for, Action, Bindings, ControlMap, Driver, InputState};

/// How far ahead of the eye an activate ray reaches, in Skyrim units. The
/// vanilla reach is about this, and it is a setting rather than a constant so
/// a later pass can read `fActivatePickLength` from the INI.
const ACTIVATE_REACH: f32 = 180.0;

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
    rig: CameraRig,
    motion: CameraMotion,
    bindings: Bindings,
    driver: Driver,
    world: Option<SimpleWorld>,
    character: Option<BodyId>,
    data_source: Option<Arc<DataSource>>,
    camera: Option<Rid>,
    /// Mouse motion accumulated since the last camera or physics update, so a frame that
    /// delivers two motion events turns twice as far rather than once.
    pending_look: (f32, f32),
    /// Whether the activate ray hit something on the last step, reported to
    /// GDScript so a prompt can be drawn without a second raycast.
    last_activate_hit: bool,
    held_actions: BTreeSet<Action>,
}

#[godot_api]
impl INode3D for PlayerNode {
    fn init(base: Base<Node3D>) -> Self {
        PlayerNode {
            base,
            capture_mouse: true,
            auto_step: true,
            rig: CameraRig::default(),
            motion: CameraMotion::new(Vec3::ZERO, CharacterSettings::default().height),
            bindings: Bindings::default(),
            driver: Driver::new(),
            world: None,
            character: None,
            data_source: None,
            camera: None,
            pending_look: (0.0, 0.0),
            last_activate_hit: false,
            held_actions: BTreeSet::new(),
        }
    }

    fn ready(&mut self) {
        self.register_actions();
        self.create_camera();
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::CAPTURED);
        }
    }

    fn unhandled_input(&mut self, event: Gd<godot::classes::InputEvent>) {
        if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
            let rel = motion.get_relative();
            self.pending_look.0 += rel.x;
            self.pending_look.1 += rel.y;
        }
    }

    fn physics_process(&mut self, delta: f64) {
        if self.auto_step {
            self.advance(delta);
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.auto_step {
            let (dx, dy) = std::mem::take(&mut self.pending_look);
            self.rig.look(dx, dy);
            let fraction = Engine::singleton().get_physics_interpolation_fraction() as f32;
            let (feet, height) = self.motion.sample(fraction);
            self.place_camera(feet, height);
        }
    }

    fn exit_tree(&mut self) {
        if self.capture_mouse {
            Input::singleton().set_mouse_mode(godot::classes::input::MouseMode::VISIBLE);
        }
        if let Some(rid) = self.camera.take() {
            RenderingServer::singleton().free_rid(rid);
        }
    }
}

#[godot_api]
impl PlayerNode {
    /// Open the game data this node reads cells from. Mirrors
    /// `CellStreamerNode::load_data`, and the two can point at the same
    /// install independently.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_data(&mut self, path: GString) -> bool {
        match DataSource::open(Path::new(&path.to_string())) {
            Ok(source) => {
                self.load_control_map(&source);
                if self.base().is_node_ready() {
                    self.register_actions();
                }
                self.data_source = Some(Arc::new(source));
                true
            }
            Err(e) => {
                godot_error!("PlayerNode::load_data: {e}");
                self.data_source = None;
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
        self.data_source = Some(source);
        true
    }

    /// Replace the keyboard bindings with defaults plus a control-map overlay.
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
        let mut world = collision.world;
        let bodies = collision.stats.bodies;
        let id = world.create_character(CharacterSettings::default(), Vec3::new(x, y, z));
        self.character = Some(id);
        self.world = Some(world);
        self.motion = CameraMotion::new(self.feet(), self.capsule_height());
        self.update_camera();
        bodies as i32
    }

    /// One simulation step. Public so a scripted run can drive a fixed step
    /// instead of relying on frame timing.
    #[func]
    pub fn advance(&mut self, delta: f64) {
        let (dx, dy) = std::mem::take(&mut self.pending_look);
        self.rig.look(dx, dy);

        let input = self.gather();
        if input.toggle_pov_pressed {
            self.rig.mode = self.rig.mode.toggled();
        }

        let camera = self.rig;
        let intent = self.driver.update(&input, &camera);

        if let (Some(world), Some(id)) = (self.world.as_mut(), self.character) {
            world.set_crouch(id, intent.crouch);
            world.walk(id, intent.direction, intent.speed);
            if intent.jump {
                world.try_jump(id);
            }
            world.step(delta as f32);
        }

        self.last_activate_hit = if input.activate_pressed {
            self.activate_ray().is_some()
        } else {
            false
        };

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

    /// Whether the last activate press found something within reach.
    #[func]
    pub fn last_activate_hit(&self) -> bool {
        self.last_activate_hit
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
        }
    }

    /// Gate multi-key bindings on every key, then detect their action edge.
    /// InputMap alone represents only the last key of an arbitrary chord.
    fn gather(&mut self) -> InputState {
        let input = Input::singleton();
        let held_now: BTreeSet<Action> = Action::all()
            .iter()
            .copied()
            .filter(|&a| {
                let Some(binding) = self.bindings.get(a) else {
                    return false;
                };
                if binding.0.iter().any(|c| c.0.len() > 1) {
                    binding.held(&|vk| {
                        godot_key_for(vk)
                            .is_some_and(|k| input.is_key_pressed(Key::from_ord(k as i32)))
                    })
                } else {
                    input.is_action_pressed(&StringName::from(a.event_name()))
                }
            })
            .collect();
        let mut observed = held_now.clone();
        for action in Action::all() {
            let has_chord = self
                .bindings
                .get(action)
                .is_some_and(|b| b.0.iter().any(|c| c.0.len() > 1));
            if !has_chord && input.is_action_just_pressed(&StringName::from(action.event_name())) {
                observed.insert(action);
            }
        }
        let held = |a: Action| held_now.contains(&a);
        let pressed = |a: Action| observed.contains(&a) && !self.held_actions.contains(&a);
        let axis = |neg: Action, pos: Action| {
            f32::from(u8::from(held(pos))) - f32::from(u8::from(held(neg)))
        };
        let state = InputState {
            move_x: axis(Action::StrafeLeft, Action::StrafeRight),
            move_y: axis(Action::Back, Action::Forward),
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
        self.held_actions = observed;
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

    fn create_camera(&mut self) {
        let mut server = RenderingServer::singleton();
        let camera = server.camera_create();
        if let Some(viewport) = self.viewport_rid() {
            server.viewport_attach_camera(viewport, camera);
        } else {
            godot_error!("PlayerNode: no viewport to attach a camera to");
        }
        self.camera = Some(camera);
        self.update_camera();
    }

    fn viewport_rid(&self) -> Option<Rid> {
        self.base()
            .get_viewport()
            .map(|v: Gd<Viewport>| v.get_viewport_rid())
    }

    /// Place the camera where the rig says, converting into Godot's space.
    fn update_camera(&mut self) {
        self.place_camera(self.feet(), self.capsule_height());
    }

    fn place_camera(&mut self, feet: Vec3, height: f32) {
        let Some(camera) = self.camera else {
            return;
        };

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
        let mut server = RenderingServer::singleton();
        server.camera_set_transform(camera, transform);
        server.camera_set_perspective(camera, self.rig.fov(), 0.05, 4096.0);
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
