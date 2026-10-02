// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::batch::cell_occluder_mesh;
use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
use soe_godot_host::mesh::{build_surface, AlphaMode, MaterialDesc, MeshData, MeshOptions};

#[test]
fn rnd_07_skinned_actor_geometry_is_not_used_as_a_static_occluder() {
    let data = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        bone_indices: Some(vec![[0, 0, 0, 0]; 3]),
        bone_weights: Some(vec![[1.0, 0.0, 0.0, 0.0]; 3]),
        material: MaterialDesc {
            alpha: AlphaMode::Opaque,
            ..MaterialDesc::default()
        },
        ..MeshData::default()
    };
    let surface = LoadedSurface {
        reference: GlobalFormId { file: 0, object: 1 },
        arrays: build_surface(&data, MeshOptions::default()).unwrap(),
        textures: SurfaceTextures::default(),
        placement: soe_physics::Iso::IDENTITY,
        instance_transform: None,
        mesh_identity: None,
    };

    assert!(surface.arrays.is_skinned());
    assert_eq!(
        cell_occluder_mesh(&[surface]),
        (Vec::new(), Vec::new()),
        "Godot's static occlusion guidance excludes animated actor geometry"
    );
}
