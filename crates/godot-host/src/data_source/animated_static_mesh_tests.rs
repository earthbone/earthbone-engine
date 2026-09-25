// SPDX-License-Identifier: GPL-3.0-or-later

//! RND-08 at the host layer: `AnimatedStaticNode::load_nif` walks
//! `soe_nif::mesh::meshes_under_children` from the animated node, not the
//! file root, so a child mesh's baked geometry must carry only its own
//! placement relative to the node, never the node's own authored transform,
//! which `advance` applies separately every frame from the evaluated
//! controller. A test that walked from the root instead would not catch a
//! regression back to double counting, since both would then agree.
//!
//! Uses the same `fixture` writer as `placement_tests` and `emission_tests`
//! next door, and drives `super::mesh_data_from` and `crate::mesh::build_surface`
//! directly, the exact pipeline `godot_animated_static::load_nif` calls.

use super::fixture;

use fixture::{quad, NifWriter, IDENTITY};
use soe_nif::mesh::meshes_under_children;
use soe_nif::Vec3;

use crate::animated_static::find_node;
use crate::mesh::{build_surface, MeshOptions};

/// Root, "Target" (an `NiNode`, translated `(5, 0, 0)`, no controller, one
/// is not needed for a mesh placement check), one `BSTriShape` child of
/// Target translated `(2, 0, 0)` in Target's own local space.
fn scene() -> Vec<u8> {
    let mut w = NifWriter::new(100);
    let (vertices, triangles) = quad();
    let shape = w.bs_tri_shape(
        "Shape",
        [2.0, 0.0, 0.0],
        -1,
        -1,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = w.add("BSTriShape", shape);
    let target = w.ni_node("Target", [5.0, 0.0, 0.0], IDENTITY, 1.0, &[shape]);
    let target = w.add("NiNode", target);
    let root = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[target]);
    let root = w.add("NiNode", root);
    w.roots = vec![root];
    w.finish()
}

#[test]
fn rnd_08_meshes_under_children_excludes_the_animated_nodes_own_placement() {
    let bytes = scene();
    let file = soe_nif::parse(&bytes).expect("the scene parses");
    let target = find_node(&file, "Target").expect("Target resolves");

    let under_target = meshes_under_children(&file, target).expect("walks from Target");
    assert_eq!(under_target.len(), 1);
    // Only the shape's own local offset, Target's (5, 0, 0) is left out:
    // `advance` supplies it live from the evaluated controller instead.
    assert_eq!(
        under_target[0].transform.apply(Vec3::ZERO),
        Vec3::new(2.0, 0.0, 0.0)
    );

    // The whole-file walk from the root is the control: it does compose
    // Target's own placement, so the two walks must disagree, otherwise
    // this test would pass even if `meshes_under_children` regressed to
    // walking from the root too.
    let from_root = file.meshes().expect("walks from the root");
    assert_eq!(from_root.len(), 1);
    assert_eq!(
        from_root[0].transform.apply(Vec3::ZERO),
        Vec3::new(7.0, 0.0, 0.0)
    );
    assert_ne!(under_target[0].transform, from_root[0].transform);
}

#[test]
fn rnd_08_load_nif_pipeline_bakes_and_builds_a_surface_for_the_child_mesh() {
    let bytes = scene();
    let file = soe_nif::parse(&bytes).expect("the scene parses");
    let target = find_node(&file, "Target").expect("Target resolves");
    let meshes = meshes_under_children(&file, target).expect("walks from Target");
    let mesh = &meshes[0];

    // The exact two calls `godot_animated_static::load_nif` makes per child
    // mesh, minus the Godot resource wrapping `array_mesh_from` does, which
    // needs a live engine and is proven instead by the Godot end to end
    // check (`godot/rnd_08_test.gd`).
    let data = super::mesh_data_from(mesh, &mesh.transform);
    let surface = build_surface(&data, MeshOptions::default()).expect("the quad builds a surface");
    assert_eq!(surface.positions.len(), 4);
    // The baked positions carry the shape's local (2, 0, 0) offset, not
    // Target's placement, matching the transform check above. `build_surface`
    // converts to Godot space (`units::METRES_PER_UNIT`, x preserved) on the
    // way out, the same as every other consumer of its `SurfaceArrays`.
    let unit_x = 2.0 * crate::units::METRES_PER_UNIT;
    let quad_x = 3.0 * crate::units::METRES_PER_UNIT;
    assert!(surface
        .positions
        .iter()
        .all(|p| (p[0] - unit_x).abs() < 1e-5 || (p[0] - quad_x).abs() < 1e-5));
}
