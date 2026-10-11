// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::batch::cell_occluder_mesh;
use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
use soe_godot_host::mesh::{build_surface, AlphaMode, MaterialDesc, MeshData, MeshOptions};
use soe_godot_host::units::{GodotPos, GodotTransform};

fn opaque_surface(id: u32, triangles: usize) -> LoadedSurface {
    let data = MeshData {
        positions: vec![[1.0, 2.0, 3.0], [2.0, 2.0, 3.0], [1.0, 3.0, 3.0]],
        indices: (0..triangles).flat_map(|_| [0, 1, 2]).collect(),
        material: MaterialDesc {
            alpha: AlphaMode::Opaque,
            ..MaterialDesc::default()
        },
        ..MeshData::default()
    };
    LoadedSurface {
        animation: None,
        reference: GlobalFormId {
            file: 0,
            object: id,
        },
        arrays: build_surface(&data, MeshOptions::default()).unwrap(),
        textures: SurfaceTextures::default(),
        placement: soe_physics::Iso::IDENTITY,
        instance_transform: None,
        mesh_identity: None,
    }
}

#[test]
fn rnd_07_multimesh_rotation_and_scale_are_applied_to_each_occluder_vertex() {
    let mut candidate = opaque_surface(1, 1);
    candidate.instance_transform = Some(GodotTransform {
        basis: [[0.0, -2.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]],
        origin: GodotPos::new(4.0, 5.0, 6.0),
    });

    let (vertices, indices) = cell_occluder_mesh(&[candidate]);

    assert_eq!(
        vertices,
        [
            [3.914_275, 5.028_575, 5.942_85],
            [3.914_275, 5.057_15, 5.942_85],
            [3.914_275, 5.028_575, 5.914_275],
        ]
    );
    assert_eq!(indices, [0, 2, 1]);
}

#[test]
fn rnd_07_combined_mesh_indices_remain_in_range_at_selection_boundary() {
    let surfaces = [
        opaque_surface(1, 1),
        opaque_surface(2, 2),
        opaque_surface(3, 3),
        opaque_surface(4, 4),
        opaque_surface(5, 5),
    ];

    let (vertices, indices) = cell_occluder_mesh(&surfaces);

    assert_eq!(vertices.len(), 12, "only four source surfaces are selected");
    assert_eq!(
        indices.len(),
        42,
        "all triangles from the selected sources remain"
    );
    assert!(indices
        .iter()
        .all(|index| usize::try_from(*index).is_ok_and(|index| index < vertices.len())));
}
