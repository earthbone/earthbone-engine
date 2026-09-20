// SPDX-License-Identifier: GPL-3.0-or-later

//! Placed-model transforms checked with the NIF crate's independent fixture writer.

use super::fixture;

use fixture::{quad, NifWriter, ROT_Z_90};
use soe_nif::{blocks::Block, vertex::attr, BlockRef, Transform, Vec3};

fn scene(root_name: &str, roots: &[i32]) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let child = writer.reserve("NiNode");
    let (vertices, triangles) = quad();
    let shape = writer.bs_tri_shape(
        "Shape",
        [3.0, 0.0, 0.0],
        -1,
        -1,
        -1,
        attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let child_bytes = writer.ni_node("Child", [2.0, 0.0, 0.0], ROT_Z_90, 1.0, &[shape]);
    writer.fill(child, child_bytes);
    let root_bytes = writer.ni_node(root_name, [10.0, 0.0, 0.0], ROT_Z_90, 2.0, &[child]);
    writer.fill(root, root_bytes);
    writer.roots = roots.to_vec();
    writer.finish()
}

#[test]
fn placed_root_is_reset_before_child_rotation_and_translation_compose() {
    let file = super::placed_model(&scene("Root", &[0])).unwrap();
    let meshes = file.meshes().unwrap();
    let origin = meshes[0].transform.apply(Vec3::ZERO);
    assert_eq!(origin, Vec3::new(2.0, -3.0, 0.0));
    assert_eq!(
        meshes[0].transform.apply(Vec3::new(1.0, 0.0, 0.0)),
        Vec3::new(2.0, -4.0, 0.0)
    );
}

#[test]
fn bip01_keeps_authored_root_placement_and_scale() {
    let file = super::placed_model(&scene("bIP01", &[0])).unwrap();
    let mesh = file.meshes().unwrap().remove(0);
    assert_eq!(mesh.transform.apply(Vec3::ZERO), Vec3::new(4.0, -4.0, 0.0));
}

#[test]
fn multiple_roots_keep_their_authored_placement() {
    let file = super::placed_model(&scene("Root", &[0, 1])).unwrap();
    let Block::Node(root) = file.get(BlockRef(0)).unwrap() else {
        panic!("root node");
    };
    assert_eq!(root.av.transform.translation, Vec3::new(10.0, 0.0, 0.0));
    assert_eq!(root.av.transform.scale, 2.0);
    assert_ne!(root.av.transform, Transform::IDENTITY);
}
