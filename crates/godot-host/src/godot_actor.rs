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

use godot::classes::{ArrayMesh, INode3D, Node3D, RenderingServer};
use godot::prelude::*;
use soe_bsa::Vfs;
use soe_hkx::interp::Interpreter;
use soe_hkx::pose::to_model_space_into;
use soe_hkx::QsTransform;

use crate::actor::{ActorAssets, ActorSurface, HUMAN_SKELETON};
use crate::data_source::DataSource;
use crate::godot_layer::array_mesh_from;
use crate::skinned::{skinning_transforms_into, Affine};

/// The archives a character reads from, in load order.
const ARCHIVES: [&str; 3] = [
    "Skyrim - Meshes0.bsa",
    "Skyrim - Meshes1.bsa",
    "Skyrim - Animations.bsa",
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
    /// Reused every frame so a tick allocates nothing.
    model: Vec<QsTransform>,
    skins: Vec<Affine>,
    stats: VarDictionary,
    elapsed: f64,
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
            model: Vec::new(),
            skins: Vec::new(),
            stats: VarDictionary::new(),
            elapsed: 0.0,
        }
    }

    fn exit_tree(&mut self) {
        self.free_rids();
    }

    fn process(&mut self, delta: f64) {
        if !self.playing {
            self.refresh_appearance();
            return;
        }
        self.advance(delta as f32);
    }
}

#[godot_api]
impl SkinnedActorNode {
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
        if self.assets.is_some() {
            godot_error!("SkinnedActorNode: already loaded, make another node");
            return false;
        }
        let dir = data_dir.to_string();
        let mut vfs = Vfs::new();
        for name in ARCHIVES {
            let path = Path::new(&dir).join(name);
            if let Err(e) = vfs.add_archive_path(&path) {
                godot_error!("SkinnedActorNode: {}: {e}", path.display());
                return false;
            }
        }
        let mut assets = match ActorAssets::load(&vfs, &mesh_path.to_string()) {
            Ok(a) => a,
            Err(e) => {
                godot_error!("SkinnedActorNode: {e}");
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
            let mesh = array_mesh_from(&surface.arrays);
            let instance = server.instance_create2(mesh.get_rid(), scenario);
            server.instance_set_transform(instance, Transform3D::IDENTITY);
            server.instance_attach_skeleton(instance, skeleton);
            self.shapes.push(ShapeRids { instance, skeleton });
            self.meshes.push(mesh);
        }
    }

    fn update_surface_stats(&mut self) {
        self.stats.set("shapes", self.surfaces.len() as i64);
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

    /// Advance by `delta` seconds and push the new pose. `_process` calls
    /// this while `playing`, and a test can call it directly.
    #[func]
    pub fn advance(&mut self, delta: f32) {
        self.refresh_appearance();
        let (Some(assets), Some(vm)) = (self.assets, self.vm.as_mut()) else {
            return;
        };
        let _span = soe_trace::Span::new("host.actor_tick", "host");
        vm.update(delta);
        self.elapsed += f64::from(delta);
        if let Err(e) = to_model_space_into(&assets.skeleton, vm.pose(), &mut self.model) {
            godot_error!("SkinnedActorNode: {e}");
            return;
        }
        self.push_pose();
    }

    fn push_pose(&mut self) {
        let mut server = RenderingServer::singleton();
        for (surface, rids) in self.surfaces.iter().zip(&self.shapes) {
            if let Err(e) =
                skinning_transforms_into(&surface.map, &surface.skin, &self.model, &mut self.skins)
            {
                godot_error!("SkinnedActorNode: {e}");
                return;
            }
            for (i, skin) in self.skins.iter().enumerate() {
                server.skeleton_bone_set_transform(rids.skeleton, i as i32, to_transform3d(skin));
            }
        }
    }

    /// What the load found: shape, bone and clip counts. Empty before a load.
    #[func]
    pub fn stats(&self) -> VarDictionary {
        self.stats.clone()
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
        GString::from(HUMAN_SKELETON)
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
    }
}

impl SkinnedActorNode {
    /// Bind an existing rig to a live actor. Geometry is replaced only after
    /// every selected model loads, so failed refreshes preserve the last view.
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
        self.replace_surfaces(update.surfaces)?;
        self.appearance = Some(AppearanceBinding {
            world,
            source,
            actor,
            cache,
        });
        Ok(())
    }

    fn refresh_appearance(&mut self) {
        let (Some(binding), Some(assets)) = (&mut self.appearance, self.assets) else {
            return;
        };
        let update = match binding.cache.prepare(
            &mut binding.world.borrow_mut(),
            binding.actor,
            binding.source.vfs(),
            &assets.skeleton,
        ) {
            Ok(Some(update)) => update,
            Ok(None) => return,
            Err(error) => {
                godot_error!("SkinnedActorNode appearance: {error}");
                return;
            }
        };
        if let Err(error) = self.replace_surfaces(update.surfaces) {
            godot_error!("SkinnedActorNode appearance: {error}");
        }
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
