// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::batch::{cell_occluder_mesh, CELL_OCCLUDER_SURFACES};
use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
use soe_godot_host::mesh::{build_surface, AlphaMode, MaterialDesc, MeshData, MeshOptions};
use soe_godot_host::units::{GodotPos, GodotTransform};

fn surface(id: u32, triangles: usize, alpha: AlphaMode) -> LoadedSurface {
    let data = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: (0..triangles).flat_map(|_| [0, 1, 2]).collect(),
        material: MaterialDesc {
            alpha,
            ..MaterialDesc::default()
        },
        ..MeshData::default()
    };
    LoadedSurface {
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
fn rnd_07_empty_and_nonopaque_cells_produce_no_occluder() {
    assert_eq!(cell_occluder_mesh(&[]), (Vec::new(), Vec::new()));
    let surfaces = [
        surface(1, 3, AlphaMode::Blend),
        surface(2, 2, AlphaMode::Cutout(128)),
        surface(3, 0, AlphaMode::Opaque),
    ];
    assert_eq!(cell_occluder_mesh(&surfaces), (Vec::new(), Vec::new()));
}

#[test]
fn rnd_07_equal_sized_opaque_candidates_keep_source_order_and_valid_offsets() {
    let surfaces = [
        surface(1, 1, AlphaMode::Opaque),
        surface(2, 1, AlphaMode::Opaque),
        surface(3, 1, AlphaMode::Opaque),
        surface(4, 1, AlphaMode::Opaque),
        surface(5, 1, AlphaMode::Opaque),
    ];

    let (vertices, indices) = cell_occluder_mesh(&surfaces);

    assert_eq!(vertices.len(), CELL_OCCLUDER_SURFACES * 3);
    let expected = (0..CELL_OCCLUDER_SURFACES as i32)
        .flat_map(|triangle| [triangle * 3, triangle * 3 + 2, triangle * 3 + 1])
        .collect::<Vec<_>>();
    assert_eq!(indices, expected);
}

#[test]
fn rnd_07_multimesh_occluder_uses_the_rendered_placement() {
    let mut candidate = surface(1, 1, AlphaMode::Opaque);
    candidate.instance_transform = Some(GodotTransform {
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        origin: GodotPos::new(7.0, 8.0, 9.0),
    });

    let (vertices, _) = cell_occluder_mesh(&[candidate]);

    assert_eq!(vertices[0], [7.0, 8.0, 9.0]);
}
