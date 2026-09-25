// SPDX-License-Identifier: GPL-3.0-or-later

//! A node that plays an authored NIF transform controller on real scene tree
//! children.
//!
//! Unlike `godot_actor` and the cell streamer next door, which bypass the
//! scene tree for the update volume a character or a whole cell need, an
//! animated static is one node with a handful of child meshes, so a real
//! `MeshInstance3D` per child mesh keeps the wiring small and makes the
//! result trivially readable from a test script through `Node3D.transform`.
//!
//! `load_nif` resolves the node's controller chain through
//! [`crate::animated_static`] and builds one static child mesh per shape
//! under it, in the node's own local space
//! (`soe_nif::mesh::meshes_under_children`). `advance` evaluates the
//! controller at the node's own elapsed time and writes the result onto
//! every child, the same way `_process` would each frame. The math lives in
//! `soe_nif::controller::evaluate_transform` and `animated_static`, already
//! covered by their own tests, this module only wires them to the engine.

use std::path::Path;

use godot::classes::{INode3D, MeshInstance3D, Node3D};
use godot::prelude::*;

use crate::animated_static::{self, AnimatedController};
use crate::data_source::mesh_data_from;
use crate::godot_layer::array_mesh_from;
use crate::mesh::{build_surface, MeshOptions};
use crate::skinned::Affine;

/// Godot's `Basis` is column major in its constructor, and `Affine::basis` is
/// row major, so each column is a column of the matrix. Kept in step with
/// `godot_actor::to_transform3d`, which does the same conversion for a
/// skinned bone, rather than made `pub(crate)` there: the two call sites have
/// nothing else in common and duplicating six lines costs less than the
/// coupling.
fn to_transform3d(a: &Affine) -> Transform3D {
    Transform3D::new(
        Basis::from_cols(
            Vector3::new(a.basis[0][0], a.basis[1][0], a.basis[2][0]),
            Vector3::new(a.basis[0][1], a.basis[1][1], a.basis[2][1]),
            Vector3::new(a.basis[0][2], a.basis[1][2], a.basis[2][2]),
        ),
        Vector3::new(a.origin[0], a.origin[1], a.origin[2]),
    )
}

/// One authored animated static. Call `load_nif`, then let it process.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct AnimatedStaticNode {
    base: Base<Node3D>,
    /// Advance and push the new transform every frame. Off leaves the last
    /// pose standing, which is what a capture wants.
    #[export]
    playing: bool,
    controller: Option<AnimatedController>,
    children: Vec<Gd<MeshInstance3D>>,
    elapsed: f32,
}

#[godot_api]
impl INode3D for AnimatedStaticNode {
    fn init(base: Base<Node3D>) -> Self {
        AnimatedStaticNode {
            base,
            playing: true,
            controller: None,
            children: Vec::new(),
            elapsed: 0.0,
        }
    }

    fn process(&mut self, delta: f64) {
        if self.playing {
            self.advance(delta as f32);
        }
    }
}

#[godot_api]
impl AnimatedStaticNode {
    /// Load `nif_path`, find the node named `node_name`, resolve its
    /// authored transform controller, and add one `MeshInstance3D` child per
    /// mesh under it. Returns false and logs on failure. The node must
    /// already be in the tree, children cannot be added before that.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_nif(&mut self, nif_path: GString, node_name: GString) -> bool {
        if self.controller.is_some() {
            godot_error!("AnimatedStaticNode: already loaded, make another node");
            return false;
        }
        let path = nif_path.to_string();
        let bytes = match std::fs::read(Path::new(&path)) {
            Ok(b) => b,
            Err(e) => {
                godot_error!("AnimatedStaticNode: {path}: {e}");
                return false;
            }
        };
        let file = match soe_nif::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                godot_error!("AnimatedStaticNode: {path}: {e}");
                return false;
            }
        };
        let name = node_name.to_string();
        let Some(node) = animated_static::find_node(&file, &name) else {
            godot_error!("AnimatedStaticNode: no node named {name} in {path}");
            return false;
        };
        let controller = match animated_static::resolve_controller(&file, node) {
            Ok(c) => c,
            Err(e) => {
                godot_error!("AnimatedStaticNode: {name}: {e}");
                return false;
            }
        };
        let meshes = match soe_nif::mesh::meshes_under_children(&file, node) {
            Ok(m) => m,
            Err(e) => {
                godot_error!("AnimatedStaticNode: {name}: {e}");
                return false;
            }
        };
        for mesh in &meshes {
            // `mesh.transform` accumulates from identity at `node`
            // (`meshes_under_children`'s own contract), so this bakes each
            // child's placement relative to the animated node, never the
            // node's own live transform, which `advance` applies separately.
            let data = mesh_data_from(mesh, &mesh.transform);
            let surface = match build_surface(&data, MeshOptions::default()) {
                Ok(s) => s,
                Err(e) => {
                    let mesh_name = &mesh.name;
                    godot_error!("AnimatedStaticNode: {name}: {mesh_name}: {e}");
                    continue;
                }
            };
            let array_mesh = array_mesh_from(&surface);
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&array_mesh);
            self.base_mut().add_child(&instance);
            self.children.push(instance);
        }
        self.controller = Some(controller);
        self.elapsed = 0.0;
        true
    }

    /// Advance by `delta` seconds and, when the controller is active, push
    /// its transform at the new elapsed time onto every child mesh.
    /// `_process` calls this while `playing`, and a test can call it
    /// directly.
    #[func]
    pub fn advance(&mut self, delta: f32) {
        let Some(controller) = &self.controller else {
            return;
        };
        self.elapsed += delta;
        if !animated_static::is_active(controller) {
            return;
        }
        let evaluated = animated_static::evaluate(controller, self.elapsed);
        let transform = to_transform3d(&Affine::from_nif(&evaluated).to_godot());
        for child in &mut self.children {
            child.set_transform(transform);
        }
    }

    /// Seconds advanced since the last successful load.
    #[func]
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Number of mesh children the last load built, for a test to check
    /// something actually loaded.
    #[func]
    pub fn mesh_count(&self) -> i32 {
        self.children.len() as i32
    }
}
