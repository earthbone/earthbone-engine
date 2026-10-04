// SPDX-License-Identifier: GPL-3.0-or-later

//! A node that plays a vanilla character's animation on a skinned mesh.
//!
//! Like the cell streamer next door, this bypasses the scene tree: the mesh is
//! an `ArrayMesh` resource and a `RenderingServer` instance, and the skeleton
//! is a `RenderingServer` skeleton RID rather than a `Skeleton3D` node with a
//! `Skin` resource. At that level a bone's transform **is** the final skinning
//! matrix, pose combined with inverse bind, which is exactly what
//! [`crate::skinned::skinning_transforms_into`] produces. Nothing here does
//! arithmetic, it copies.
//!
//! Every frame: tick the interpreter, lift its local pose into model space,
//! combine with the mesh's bind pose, copy into the skeleton. The two buffers
//! are kept on the node so a frame allocates nothing.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use godot::classes::{ArrayMesh, INode3D, Material, Node3D, RenderingServer};
use godot::prelude::*;
use soe_bsa::Vfs;
use soe_game::combat::{Error as CombatError, Hand};
use soe_hkx::interp::Interpreter;
use soe_hkx::pose::to_model_space_into;
use soe_hkx::QsTransform;

use crate::actor::{ActorAssets, ActorSurface};
use crate::animation_dispatch::{report_graph_events, AnimationEventCursor};
use crate::data_source::DataSource;
use crate::godot_layer::actor_array_mesh_from;
use crate::godot_ragdoll::LiveRagdoll;
use crate::ragdoll::{ActorRenderMode, RagdollBoneMap};
use crate::skinned::{skinning_transforms_into, Affine};

/// The archives a character reads from, in load order.
const ARCHIVES: [&str; 12] = [
    "Skyrim - Meshes0.bsa",
    "Skyrim - Meshes1.bsa",
    "Skyrim - Animations.bsa",
    "Skyrim - Textures0.bsa",
    "Skyrim - Textures1.bsa",
    "Skyrim - Textures2.bsa",
    "Skyrim - Textures3.bsa",
    "Skyrim - Textures4.bsa",
    "Skyrim - Textures5.bsa",
    "Skyrim - Textures6.bsa",
    "Skyrim - Textures7.bsa",
    "Skyrim - Textures8.bsa",
];

fn to_transform3d(a: &Affine) -> Transform3D {
    // Godot's Basis is column major in its constructor, and `a.basis` is row
    // major, so each column is a column of the matrix.
    Transform3D::new(
        Basis::from_cols(
            Vector3::new(a.basis[0][0], a.basis[1][0], a.basis[2][0]),
            Vector3::new(a.basis[0][1], a.basis[1][1], a.basis[2][1]),
            Vector3::new(a.basis[0][2], a.basis[1][2], a.basis[2][2]),
        ),
        Vector3::new(a.origin[0], a.origin[1], a.origin[2]),
    )
}

/// The two RIDs one skinned shape needs.
struct ShapeRids {
    instance: Rid,
    skeleton: Rid,
}

struct AppearanceBinding {
    world: Rc<RefCell<soe_game::World>>,
    source: Arc<DataSource>,
    actor: soe_esm::GlobalFormId,
    cache: crate::actor::AppearanceCache,
    stagger: StaggerTracker,
    animation_cursor: AnimationEventCursor,
}

/// A graph event derived from the bound actor's shared stagger deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaggerGraphEvent {
    Start,
    Stop,
}

/// Send the shared actor's stagger magnitude and start event to its graph.
pub fn send_stagger_start(vm: &mut Interpreter<'_>, magnitude: f32) -> bool {
    vm.variables.set_by_name("staggerMagnitude", magnitude);
    vm.send_event_named("staggerStart")
}

/// Tracks which stagger transition the actor graph has already received.
#[derive(Default)]
pub struct StaggerTracker {
    last_deadline: Option<f64>,
    active: bool,
}

impl StaggerTracker {
    /// Read the bound actor's stagger state from the shared game world.
    pub fn poll(
        &mut self,
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
    ) -> Option<(StaggerGraphEvent, f32)> {
        let state = world
            .actor(actor)
            .map(|actor| (actor.stagger_end_time, actor.stagger_magnitude));
        let (deadline, magnitude) = state?;
        self.observe(deadline, world.clock.real_seconds)
            .map(|event| (event, magnitude as f32))
    }

    fn observe(&mut self, deadline: Option<f64>, now: f64) -> Option<StaggerGraphEvent> {
        if self.last_deadline != deadline {
            self.last_deadline = deadline;
            if deadline.is_some_and(|until| until > now) {
                self.active = true;
                return Some(StaggerGraphEvent::Start);
            }
        }
        if self.active && deadline.is_none_or(|until| until <= now) {
            self.active = false;
            return Some(StaggerGraphEvent::Stop);
        }
        None
    }
}

/// One animated character. Call `load_character`, then let it process.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct SkinnedActorNode {
    base: Base<Node3D>,
    /// Tick the graph and push bones every frame. Off leaves the last pose
    /// standing, which is what a capture wants.
    #[export]
    playing: bool,
    /// The character's files. Leaked on purpose, see `load_character`.
    assets: Option<&'static ActorAssets>,
    vm: Option<Interpreter<'static>>,
    surfaces: Vec<ActorSurface>,
    appearance: Option<AppearanceBinding>,
    /// One instance and one skeleton per shape. They cannot share a skeleton:
    /// a shape's vertex bone indices address its **own** bone list, so two
    /// shapes naming different bones would read each other's transforms.
    shapes: Vec<ShapeRids>,
    meshes: Vec<Gd<ArrayMesh>>,
    materials: Vec<Gd<Material>>,
    /// Reused every frame so a tick allocates nothing.
    model: Vec<QsTransform>,
    skins: Vec<Affine>,
    uploaded_skins: Vec<Transform3D>,
    stats: VarDictionary,
    elapsed: f64,
    invisible: bool,
    ragdoll_map: Option<RagdollBoneMap>,
    ragdoll: Option<LiveRagdoll>,
}

#[godot_api]
impl INode3D for SkinnedActorNode {
    fn init(base: Base<Node3D>) -> Self {
        SkinnedActorNode {
            base,
            playing: true,
            assets: None,
            vm: None,
            surfaces: Vec::new(),
            appearance: None,
            shapes: Vec::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            model: Vec::new(),
            skins: Vec::new(),
            uploaded_skins: Vec::new(),
            stats: VarDictionary::new(),
            elapsed: 0.0,
            invisible: false,
            ragdoll_map: None,
            ragdoll: None,
        }
    }

    fn exit_tree(&mut self) {
        if let Some(binding) = &self.appearance {
            binding
                .world
                .borrow_mut()
                .queue_animation_unload(binding.actor);
        } else if self.vm.is_some() {
            self.signals().animation_graph_unloaded().emit();
        }
        if let Some(mut ragdoll) = self.ragdoll.take() {
            ragdoll.free();
        }
        self.free_rids();
    }

    fn process(&mut self, delta: f64) {
        self.sync_death_rendering();
        self.apply_queued_impulses();
        if self.ragdoll.is_some() {
            self.refresh_appearance();
            self.push_pose();
            return;
        }
        if !self.playing {
            self.sync_invisibility();
            self.refresh_appearance();
            return;
        }
        self.advance(delta as f32);
    }
}

#[godot_api]
impl SkinnedActorNode {
    /// Authored graph events emitted after the corresponding pose is installed.
    #[signal]
    pub fn animation_event(name: GString);

    /// The loaded behavior graph is leaving the scene tree.
    #[signal]
    pub fn animation_graph_unloaded();

    /// Load the vanilla human character from `data_dir` and put `mesh_path`
    /// on its skeleton. Returns false and logs on failure.
    ///
    /// The assets are leaked rather than stored, because the interpreter
    /// borrows the graph and the clips for as long as it runs and a node
    /// cannot hold both halves of that borrow. One character's files are a
    /// few megabytes and live for the process, so this is bounded: a second
    /// call on the same node is refused rather than leaking again.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_character(&mut self, data_dir: GString, mesh_path: GString) -> bool {
        self.load_character_with_behavior(
            &data_dir.to_string(),
            &mesh_path.to_string(),
            crate::actor::MASTER_BEHAVIOR,
        )
    }

    /// Load the vanilla one-handed attack graph on this character rig.
    /// Its authored markers are emitted through `animation_event`.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn load_melee_character(&mut self, data_dir: GString, mesh_path: GString) -> bool {
        self.load_character_with_behavior(
            &data_dir.to_string(),
            &mesh_path.to_string(),
            crate::actor::ONE_HAND_BEHAVIOR,
        )
    }

    fn load_character_with_behavior(
        &mut self,
        data_dir: &str,
        mesh_path: &str,
        behavior: &str,
    ) -> bool {
        if self.assets.is_some() {
            soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode: already loaded, make another node");
            return false;
        }
        let mut vfs = Vfs::new();
        for name in ARCHIVES {
            let path = Path::new(data_dir).join(name);
            if let Err(e) = vfs.add_archive_path(&path) {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode: {}: {e}", path.display());
                return false;
            }
        }
        let mut assets = match ActorAssets::load_with_behavior(&vfs, mesh_path, behavior) {
            Ok(a) => a,
            Err(e) => {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode: {e}");
                return false;
            }
        };
        let Some(scenario) = self.scenario() else {
            return false;
        };
        self.surfaces = std::mem::take(&mut assets.surfaces);
        let assets: &'static ActorAssets = Box::leak(Box::new(assets));
        self.upload_surfaces(scenario);

        let mut stats = VarDictionary::new();
        stats.set("skeleton_bones", assets.skeleton.bones.len() as i64);
        stats.set("graph_nodes", assets.graph.nodes.len() as i64);
        stats.set("clips", assets.clips.len() as i64);
        stats.set("missing_clips", assets.missing_clips.len() as i64);
        self.stats = stats;
        self.update_surface_stats();

        self.vm = Some(Interpreter::new(
            &assets.graph,
            &assets.clips,
            assets.skeleton.reference_pose.clone(),
        ));
        self.assets = Some(assets);
        self.ragdoll_map = assets
            .ragdoll
            .as_ref()
            .map(|ragdoll| RagdollBoneMap::new(&assets.skeleton, ragdoll));
        true
    }

    fn upload_surfaces(&mut self, scenario: Rid) {
        let mut server = RenderingServer::singleton();
        for surface in &self.surfaces {
            let bones = surface.map.len();
            let skeleton = server.skeleton_create();
            server.skeleton_allocate_data(skeleton, bones as i32);
            for i in 0..bones {
                server.skeleton_bone_set_transform(skeleton, i as i32, Transform3D::IDENTITY);
            }
            let mut mesh = actor_array_mesh_from(&surface.arrays, &surface.expression_shapes);
            let material =
                crate::godot_layer::material_from(&surface.arrays.material, &surface.textures);
            mesh.surface_set_material(0, &material);
            let instance = server.instance_create2(mesh.get_rid(), scenario);
            server.instance_set_transform(instance, Transform3D::IDENTITY);
            server.instance_set_visible(instance, visible_for_invisibility(self.invisible));
            server.instance_attach_skeleton(instance, skeleton);
            self.shapes.push(ShapeRids { instance, skeleton });
            self.meshes.push(mesh);
            self.materials.push(material);
        }
    }

    fn update_surface_stats(&mut self) {
        self.stats.set("shapes", self.surfaces.len() as i64);
        self.stats.set("invisible", self.invisible);
        self.stats.set("skeletons", self.surfaces.len() as i64);
        self.stats.set(
            "mesh_bones",
            self.surfaces.iter().map(|s| s.map.len()).sum::<usize>() as i64,
        );
        self.stats.set(
            "matched_bones",
            self.surfaces.iter().map(|s| s.map.matched()).sum::<usize>() as i64,
        );
        self.stats.set(
            "partitions",
            self.surfaces.iter().map(|s| s.partitions).sum::<usize>() as i64,
        );
        self.stats.set(
            "remapped_shapes",
            self.surfaces.iter().filter(|s| s.remapped).count() as i64,
        );
        self.stats.set(
            "unmatched",
            self.surfaces
                .iter()
                .map(|s| s.map.unmatched.len())
                .sum::<usize>() as i64,
        );
        self.stats.set(
            "triangles",
            self.surfaces
                .iter()
                .map(|s| s.arrays.triangle_count())
                .sum::<usize>() as i64,
        );
        self.stats.set(
            "expression_shapes",
            self.meshes
                .iter()
                .map(|mesh| mesh.get_blend_shape_count() as usize)
                .sum::<usize>() as i64,
        );
    }

    /// Ask the graph for a walk the way the game does: raise the speed
    /// variable and send the move event.
    #[func]
    pub fn start_walking(&mut self) {
        let Some(assets) = self.assets else { return };
        let Some(vm) = self.vm.as_mut() else { return };
        if let Some(i) = vm.variables.index("Speed") {
            vm.variables.set_real(i, 1.0);
        }
        for name in &assets.graph.data.event_names {
            if name.eq_ignore_ascii_case("moveStart") {
                vm.send_event_named(name);
            }
        }
    }

    /// Send an authored event to this actor's behavior graph. The resulting
    /// animation markers are emitted after their poses are installed.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn send_graph_event(&mut self, name: GString) -> bool {
        self.vm
            .as_mut()
            .is_some_and(|vm| vm.send_event_named(&name.to_string()))
    }

    /// Set a named behavior variable before advancing the graph.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub fn set_graph_variable(&mut self, name: GString, value: f32) -> bool {
        value.is_finite()
            && self
                .vm
                .as_mut()
                .is_some_and(|vm| vm.variables.set_by_name(&name.to_string(), value))
    }

    /// Advance by `delta` seconds and push the new pose. `_process` calls
    /// this while `playing`, and a test can call it directly.
    #[func]
    pub fn advance(&mut self, delta: f32) {
        self.sync_invisibility();
        self.sync_death_rendering();
        self.refresh_appearance();
        if self.ragdoll.is_some() {
            self.push_pose();
            return;
        }
        let stagger_event = self.take_stagger_event();
        let requests = self.take_animation_requests();
        let (Some(assets), Some(vm)) = (self.assets, self.vm.as_mut()) else {
            return;
        };
        match stagger_event {
            Some((StaggerGraphEvent::Start, magnitude)) => {
                send_stagger_start(vm, magnitude);
            }
            Some((StaggerGraphEvent::Stop, _)) => {
                vm.send_event_named("staggerStop");
            }
            None => {}
        }
        let _span = soe_trace::Span::new("host.actor_tick", "host");
        for request in requests {
            vm.send_event_named(&request);
        }
        vm.update(delta);
        let mut events: Vec<String> = vm
            .take_events()
            .into_iter()
            .filter_map(|event| usize::try_from(event.id).ok())
            .filter_map(|id| assets.graph.data.event_names.get(id))
            .cloned()
            .collect();
        self.elapsed += f64::from(delta);
        if let Err(e) = to_model_space_into(&assets.skeleton, vm.pose(), &mut self.model) {
            soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode: {e}");
            return;
        }
        self.push_pose();
        // Authored events follow the pose they belong to.
        let report_target = self
            .appearance
            .as_ref()
            .map(|binding| (binding.world.clone(), binding.actor));
        report_graph_events(&mut events, |event| {
            if let Some((world, target)) = &report_target {
                world.borrow_mut().queue_animation_report(*target, event);
            }
            self.signals().animation_event().emit(&GString::from(event));
        });
    }

    fn push_pose(&mut self) {
        let placement = self
            .ragdoll
            .as_ref()
            .map_or_else(|| self.actor_placement(), LiveRagdoll::actor_transform);
        let mut server = RenderingServer::singleton();
        let facial_expression = self
            .appearance
            .as_ref()
            .and_then(|binding| binding.world.borrow_mut().facial_expression(binding.actor));
        let target = facial_expression.and_then(|(mood, _)| crate::facial::expression_name(mood));
        self.uploaded_skins.clear();
        for (surface, rids) in self.surfaces.iter().zip(&self.shapes) {
            server.instance_set_transform(rids.instance, placement);
            for (index, shape) in surface.expression_shapes.iter().enumerate() {
                let weight = if Some(shape.name.as_str()) == target {
                    f32::from(facial_expression.map_or(0, |(_, strength)| strength)) / 100.0
                } else {
                    0.0
                };
                server.instance_set_blend_shape_weight(rids.instance, index as i32, weight);
            }
            if let Err(e) =
                skinning_transforms_into(&surface.map, &surface.skin, &self.model, &mut self.skins)
            {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode: {e}");
                return;
            }
            for (i, skin) in self.skins.iter().enumerate() {
                let mut pose = to_transform3d(skin);
                if let (Some(ragdoll), Some(map), Some(Some(bone))) = (
                    &self.ragdoll,
                    &self.ragdoll_map,
                    surface.map.skeleton_index.get(i),
                ) {
                    if let Some(Some(body)) = map.animated_to_body.get(*bone) {
                        if let Some(delta) = ragdoll.local_delta(body.0) {
                            pose = delta * pose;
                        }
                    }
                }
                server.skeleton_bone_set_transform(rids.skeleton, i as i32, pose);
                self.uploaded_skins.push(pose);
            }
        }
    }

    /// What the load found: shape, bone and clip counts. Empty before a load.
    #[func]
    pub fn stats(&self) -> VarDictionary {
        self.stats.clone()
    }

    /// Show or hide this rig's RenderingServer instances with first-person geometry.
    #[func]
    pub fn set_first_person_geometry_visible(&mut self, visible: bool) {
        let mut server = RenderingServer::singleton();
        for shape in &self.shapes {
            server.instance_set_visible(shape.instance, visible);
        }
    }

    /// Whether the actor currently draws from animation or simulated bodies.
    #[func]
    pub fn render_mode(&self) -> GString {
        GString::from(if self.ragdoll.is_some() {
            "ragdoll"
        } else {
            "animated"
        })
    }

    /// The Jolt body handles owned by this actor's current death ragdoll.
    #[func]
    pub fn ragdoll_bodies(&self) -> Array<Rid> {
        self.ragdoll
            .as_ref()
            .map_or_else(Array::new, |r| r.bodies().iter().copied().collect())
    }

    /// Skin transforms most recently sent to RenderingServer, in surface order.
    #[func]
    pub fn uploaded_skin_poses(&self) -> Array<Transform3D> {
        self.uploaded_skins.iter().copied().collect()
    }

    /// Seconds of animation played since the load.
    #[func]
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// The names of the graph nodes currently in a state, for a test to show
    /// that the walk actually took.
    #[func]
    pub fn active_states(&self) -> PackedStringArray {
        let (Some(assets), Some(vm)) = (self.assets, self.vm.as_ref()) else {
            return PackedStringArray::new();
        };
        vm.active_states()
            .iter()
            .map(|(id, s)| GString::from(&format!("{}={s}", assets.graph.node(*id).name)))
            .collect()
    }

    /// The clips playing now, with their times.
    #[func]
    pub fn active_clips(&self) -> PackedStringArray {
        let (Some(assets), Some(vm)) = (self.assets, self.vm.as_ref()) else {
            return PackedStringArray::new();
        };
        vm.active_clips()
            .iter()
            .map(|(id, t)| GString::from(&format!("{}@{t:.2}", assets.graph.node(*id).name)))
            .collect()
    }

    /// The skeleton file this node loads, so a script can report it.
    #[func]
    pub fn skeleton_path(&self) -> GString {
        self.assets
            .map_or_else(GString::new, |assets| GString::from(&assets.skeleton_path))
    }

    /// The behavior graph selected for this actor's race.
    #[func]
    pub fn behavior_path(&self) -> GString {
        self.assets
            .map_or_else(GString::new, |assets| GString::from(&assets.behavior_path))
    }

    /// Named inputs and markers declared by the loaded behavior graph.
    #[func]
    pub fn graph_event_names(&self) -> PackedStringArray {
        self.assets
            .map(|assets| {
                assets
                    .graph
                    .data
                    .event_names
                    .iter()
                    .map(GString::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The armed hand's current animated weapon transform in world space.
    /// Returns an empty dictionary when the rig has no usable pose.
    #[func]
    pub fn contact_weapon_pose(&self, left_hand: bool) -> VarDictionary {
        let hand = if left_hand { Hand::Left } else { Hand::Right };
        let mut result = VarDictionary::new();
        if let Ok(pose) = self.weapon_pose(hand) {
            result.set("pose", pose);
        }
        result
    }

    /// World-space weapon node from the same animated pose sent to the
    /// RenderingServer skeleton. A bound actor follows its shared placement.
    ///
    /// # Errors
    /// The rig has no matching bone or has not installed a pose yet.
    pub(crate) fn weapon_pose(&self, hand: Hand) -> Result<Transform3D, CombatError> {
        let assets = self
            .assets
            .ok_or(CombatError::Unsupported("character rig is not loaded"))?;
        let local = crate::combat::posed_weapon_bone(&assets.skeleton, &self.model, hand)?;
        let placement = if let Some(binding) = &self.appearance {
            let actor = binding
                .world
                .borrow_mut()
                .contact_actors()
                .into_iter()
                .find(|actor| actor.actor == binding.actor)
                .ok_or(CombatError::Unsupported("animated actor is not resident"))?;
            let placed = crate::units::placement_to_godot(
                crate::units::skyrim_euler_matrix(actor.rotation),
                actor.position,
                actor.scale,
            );
            to_transform3d(&Affine {
                basis: placed.basis,
                origin: [placed.origin.x, placed.origin.y, placed.origin.z],
            })
        } else {
            self.base().get_global_transform()
        };
        Ok(placement * to_transform3d(&local))
    }

    /// The shared actor whose equipment and placement this rig displays.
    #[must_use]
    pub(crate) fn bound_actor(&self) -> Option<soe_esm::GlobalFormId> {
        self.appearance.as_ref().map(|binding| binding.actor)
    }

    /// The world to draw into, the same way `CellStreamerNode` finds it: the
    /// node's own viewport first, then the tree's root, because a `--script`
    /// run has no scene and the node's viewport can be empty.
    fn scenario(&self) -> Option<Rid> {
        crate::godot_world::scenario(&self.base(), "SkinnedActorNode")
    }

    fn free_rids(&mut self) {
        let mut server = RenderingServer::singleton();
        for shape in self.shapes.drain(..) {
            server.free_rid(shape.instance);
            server.free_rid(shape.skeleton);
        }
        self.meshes.clear();
        self.materials.clear();
    }
}

impl SkinnedActorNode {
    fn sync_invisibility(&mut self) {
        let invisible = self
            .appearance
            .as_ref()
            .is_some_and(|binding| binding.world.borrow().is_invisible(binding.actor));
        if self.invisible == invisible {
            return;
        }
        self.invisible = invisible;
        self.stats.set("invisible", invisible);
        let mut server = RenderingServer::singleton();
        for shape in &self.shapes {
            server.instance_set_visible(shape.instance, visible_for_invisibility(invisible));
        }
    }

    fn sync_death_rendering(&mut self) {
        let Some(binding) = &self.appearance else {
            return;
        };
        let mode = binding.world.borrow_mut().actor(binding.actor).map_or(
            ActorRenderMode::Animated,
            |state| {
                ActorRenderMode::for_dead(crate::ragdoll::actor_uses_ragdoll(
                    state.dead,
                    state.unconscious,
                    state.ragdoll_forced,
                ))
            },
        );
        match (mode, self.ragdoll.is_some()) {
            (ActorRenderMode::Ragdoll, false) => {
                let (Some(assets), Some(map)) = (self.assets, &self.ragdoll_map) else {
                    return;
                };
                let Some(source) = &assets.ragdoll else {
                    return;
                };
                if self.model.is_empty() {
                    if let Err(error) = to_model_space_into(
                        &assets.skeleton,
                        &assets.skeleton.reference_pose,
                        &mut self.model,
                    ) {
                        soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode ragdoll pose: {error}");
                        return;
                    }
                }
                let Some(world) = self.base().get_world_3d() else {
                    return;
                };
                let placement = self.actor_placement();
                match LiveRagdoll::create(world.get_space(), placement, source, map, &self.model) {
                    Ok(ragdoll) => self.ragdoll = Some(ragdoll),
                    Err(error) => {
                        soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode ragdoll: {error}");
                        godot_error!("SkinnedActorNode ragdoll: {error}");
                    }
                }
            }
            (ActorRenderMode::Animated, true) => {
                if let Some(mut ragdoll) = self.ragdoll.take() {
                    ragdoll.free();
                }
                self.push_pose();
            }
            _ => {}
        }
    }

    fn take_stagger_event(&mut self) -> Option<(StaggerGraphEvent, f32)> {
        let Some(binding) = &mut self.appearance else {
            return None;
        };
        let mut world = binding.world.borrow_mut();
        binding.stagger.poll(&mut world, binding.actor)
    }

    fn apply_queued_impulses(&mut self) {
        let Some(ragdoll) = &self.ragdoll else {
            return;
        };
        let Some(binding) = &self.appearance else {
            return;
        };
        let impulses = binding
            .world
            .borrow_mut()
            .take_actor_impulses(binding.actor);
        for impulse in impulses {
            ragdoll.apply_impulse(impulse.vector);
        }
    }

    fn actor_placement(&self) -> Transform3D {
        let Some(binding) = &self.appearance else {
            return self.base().get_global_transform();
        };
        let placement = binding
            .world
            .borrow_mut()
            .contact_actors()
            .into_iter()
            .find(|entry| entry.actor == binding.actor);
        let Some(actor) = placement else {
            return self.base().get_global_transform();
        };
        let placed = crate::units::placement_to_godot(
            crate::units::skyrim_euler_matrix(actor.rotation),
            actor.position,
            actor.scale,
        );
        to_transform3d(&Affine {
            basis: placed.basis,
            origin: [placed.origin.x, placed.origin.y, placed.origin.z],
        })
    }

    /// Bind an existing rig to a live actor. Geometry is replaced only after
    /// every selected model loads, so failed refreshes preserve the last view.
    /// The first upload uses the actor's current invisibility state.
    pub(crate) fn bind_appearance(
        &mut self,
        world: Rc<RefCell<soe_game::World>>,
        source: Arc<DataSource>,
        actor: soe_esm::GlobalFormId,
    ) -> Result<(), String> {
        let assets = self
            .assets
            .ok_or("load the character rig before binding its appearance")?;
        let mut cache = crate::actor::AppearanceCache::default();
        let update = cache
            .prepare(
                &mut world.borrow_mut(),
                actor,
                source.vfs(),
                &assets.skeleton,
            )
            .map_err(|error| error.to_string())?
            .ok_or("initial appearance was not prepared")?;
        self.invisible = world.borrow().is_invisible(actor);
        self.replace_surfaces(update.surfaces)?;
        let animation_cursor = {
            let world = world.borrow();
            AnimationEventCursor::at_end(&world.animation_requests)
        };
        self.appearance = Some(AppearanceBinding {
            world,
            source,
            actor,
            cache,
            stagger: StaggerTracker::default(),
            animation_cursor,
        });
        Ok(())
    }

    fn refresh_appearance(&mut self) {
        let Some(assets) = self.assets else {
            return;
        };
        let Some(binding) = self.appearance.as_ref() else {
            return;
        };
        let (world, source, actor) = (
            Rc::clone(&binding.world),
            Arc::clone(&binding.source),
            binding.actor,
        );
        let selected = match world.borrow_mut().actor_animation_rig(actor) {
            Ok(Some(rig)) => rig,
            Ok(None) => return,
            Err(error) => {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode race rig: {error}");
                return;
            }
        };
        if assets.actor_race != Some(selected.race) || assets.actor_female != Some(selected.female)
        {
            let mut next = match ActorAssets::load_for_actor(
                &mut world.borrow_mut(),
                actor,
                source.vfs(),
            ) {
                Ok(assets) => assets,
                Err(error) => {
                    soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode race rig: {error}");
                    return;
                }
            };
            let surfaces = std::mem::take(&mut next.surfaces);
            let next: &'static ActorAssets = Box::leak(Box::new(next));
            self.assets = Some(next);
            self.vm = Some(Interpreter::new(
                &next.graph,
                &next.clips,
                next.skeleton.reference_pose.clone(),
            ));
            self.model.clear();
            self.surfaces.clear();
            if let Some(binding) = &mut self.appearance {
                binding.cache = crate::actor::AppearanceCache::default();
            }
            if let Err(error) = self.replace_surfaces(surfaces) {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode race rig: {error}");
            }
            return;
        }
        let Some(binding) = &mut self.appearance else {
            return;
        };
        let update = match binding.cache.prepare(
            &mut binding.world.borrow_mut(),
            binding.actor,
            source.vfs(),
            &assets.skeleton,
        ) {
            Ok(Some(update)) => update,
            Ok(None) => return,
            Err(error) => {
                soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode appearance: {error}");
                return;
            }
        };
        if let Err(error) = self.replace_surfaces(update.surfaces) {
            soe_trace::error!(target: "host::actor", system = "host::actor", "SkinnedActorNode appearance: {error}");
        }
    }

    fn take_animation_requests(&mut self) -> Vec<String> {
        let Some(binding) = &mut self.appearance else {
            return Vec::new();
        };
        let world = binding.world.borrow();
        binding
            .animation_cursor
            .take_for(&world.animation_requests, binding.actor)
    }

    fn replace_surfaces(&mut self, surfaces: Vec<ActorSurface>) -> Result<(), String> {
        let scenario = self.scenario().ok_or("actor has no rendering scenario")?;
        if self.model.is_empty() {
            let assets = self.assets.ok_or("character rig is not loaded")?;
            to_model_space_into(
                &assets.skeleton,
                &assets.skeleton.reference_pose,
                &mut self.model,
            )
            .map_err(|error| error.to_string())?;
        }
        self.free_rids();
        self.surfaces = surfaces;
        self.upload_surfaces(scenario);
        self.update_surface_stats();
        self.push_pose();
        Ok(())
    }
}

fn visible_for_invisibility(invisible: bool) -> bool {
    !invisible
}

#[cfg(test)]
mod invisibility_tests {
    use super::visible_for_invisibility;

    #[test]
    fn mag_34_invisibility_hides_skinned_actor_instances() {
        assert!(!visible_for_invisibility(true));
    }

    #[test]
    fn mag_34_visible_actor_keeps_skinned_actor_instances_visible() {
        assert!(visible_for_invisibility(false));
    }
}

#[cfg(test)]
mod cmb_27_tests {
    use super::{StaggerGraphEvent, StaggerTracker};

    #[test]
    fn cmb_27_stagger_deadline_sends_start_once_and_stop_at_expiry() {
        let mut tracker = StaggerTracker::default();
        assert_eq!(tracker.observe(None, 1.0), None);
        assert_eq!(
            tracker.observe(Some(2.0), 1.0),
            Some(StaggerGraphEvent::Start)
        );
        assert_eq!(tracker.observe(Some(2.0), 1.5), None);
        assert_eq!(
            tracker.observe(Some(2.0), 2.0),
            Some(StaggerGraphEvent::Stop)
        );
        assert_eq!(tracker.observe(Some(2.0), 2.1), None);
    }

    #[test]
    fn cmb_27_a_new_hit_restarts_an_active_stagger_deadline_once() {
        let mut tracker = StaggerTracker::default();
        assert_eq!(
            tracker.observe(Some(2.0), 1.0),
            Some(StaggerGraphEvent::Start)
        );
        assert_eq!(
            tracker.observe(Some(3.0), 1.5),
            Some(StaggerGraphEvent::Start)
        );
        assert_eq!(tracker.observe(Some(3.0), 1.6), None);
    }
}
