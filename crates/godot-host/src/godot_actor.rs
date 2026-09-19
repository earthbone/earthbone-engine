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

use std::path::Path;

use godot::classes::{ArrayMesh, INode3D, Node3D, RenderingServer};
use godot::prelude::*;
use soe_bsa::Vfs;
use soe_hkx::interp::Interpreter;
use soe_hkx::pose::to_model_space_into;
use soe_hkx::QsTransform;

use crate::actor::{ActorAssets, HUMAN_SKELETON};
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
        let assets = match ActorAssets::load(&vfs, &mesh_path.to_string()) {
            Ok(a) => a,
            Err(e) => {
                godot_error!("SkinnedActorNode: {e}");
                return false;
            }
        };
        let assets: &'static ActorAssets = Box::leak(Box::new(assets));

        let Some(scenario) = self.scenario() else {
            return false;
        };

        let mut server = RenderingServer::singleton();
        for surface in &assets.surfaces {
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

        let (total, matched) = assets.bone_totals();
        let mut stats = VarDictionary::new();
        stats.set("shapes", assets.surfaces.len() as i64);
        stats.set("skeleton_bones", assets.skeleton.bones.len() as i64);
        stats.set("mesh_bones", total as i64);
        stats.set("matched_bones", matched as i64);
        stats.set("skeletons", assets.surfaces.len() as i64);
        stats.set(
            "partitions",
            assets.surfaces.iter().map(|s| s.partitions).sum::<usize>() as i64,
        );
        stats.set(
            "remapped_shapes",
            assets.surfaces.iter().filter(|s| s.remapped).count() as i64,
        );
        stats.set(
            "unmatched",
            assets
                .surfaces
                .iter()
                .map(|s| s.map.unmatched.len())
                .sum::<usize>() as i64,
        );
        stats.set("graph_nodes", assets.graph.nodes.len() as i64);
        stats.set("clips", assets.clips.len() as i64);
        stats.set("missing_clips", assets.missing_clips.len() as i64);
        stats.set(
            "triangles",
            assets
                .surfaces
                .iter()
                .map(|s| s.arrays.triangle_count())
                .sum::<usize>() as i64,
        );
        self.stats = stats;

        self.vm = Some(Interpreter::new(
            &assets.graph,
            &assets.clips,
            assets.skeleton.reference_pose.clone(),
        ));
        self.assets = Some(assets);
        true
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
        let mut server = RenderingServer::singleton();
        for (surface, rids) in assets.surfaces.iter().zip(&self.shapes) {
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
