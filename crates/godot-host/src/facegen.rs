// SPDX-License-Identifier: GPL-3.0-or-later

//! Head-part morph application and conversion to the host surface format.

use soe_nif::{HeadTri, Mesh as NifMesh};

use crate::data_source::mesh_data_from;
use crate::mesh::{build_surface, MeshError, MeshOptions, SurfaceArrays};

/// Errors while applying head morphs and preparing a renderable surface.
#[derive(Debug, thiserror::Error)]
pub enum FaceGenError {
    #[error(transparent)]
    Nif(#[from] soe_nif::Error),
    #[error(transparent)]
    Mesh(#[from] MeshError),
}

/// Apply TRI morphs to one already resolved NPC head-part mesh and build its
/// Godot-ready surface arrays. The caller resolves head-part records and
/// assets from the NPC's declared head-part list.
///
/// # Errors
/// Returns an error when a morph does not match the mesh or its surface data
/// cannot be represented by the host.
pub fn build_head_part_surface(
    mesh: &NifMesh,
    tri: &HeadTri,
    morph_weights: &[(&str, f32)],
) -> Result<SurfaceArrays, FaceGenError> {
    let morphed = tri.apply_to_mesh(mesh, morph_weights)?;
    let source = mesh_data_from(&morphed, &morphed.transform);
    Ok(build_surface(&source, MeshOptions::default())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_nif::{Bound, Material, Transform};

    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn i16le(out: &mut Vec<u8>, value: i16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn tri_bytes() -> Vec<u8> {
        let mut out = b"FRTRI003".to_vec();
        for value in [3, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0] {
            u32le(&mut out, value);
        }
        for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for component in point {
                out.extend_from_slice(&component.to_le_bytes());
            }
        }
        for index in [0, 1, 2] {
            u32le(&mut out, index);
        }
        for _ in 0..3 {
            u32le(&mut out, 0);
        }
        u32le(&mut out, 4);
        out.extend_from_slice(b"lift");
        out.extend_from_slice(&0.01f32.to_le_bytes());
        for delta in [[0, 0, 100], [0, 0, 100], [0, 0, 100]] {
            for component in delta {
                i16le(&mut out, component);
            }
        }
        u32le(&mut out, 0);
        u32le(&mut out, 0);
        out
    }

    fn head_part() -> NifMesh {
        NifMesh {
            name: "HeadPart".into(),
            block: 0,
            transform: Transform::default(),
            bound: Bound::default(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            tangents: vec![[1.0, 0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            colors: Vec::new(),
            indices: vec![0, 1, 2],
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
            skin: None,
            material: Material::default(),
        }
    }

    #[test]
    fn edt_22_declared_head_part_morph_renders_the_generated_geometry() {
        let tri = soe_nif::parse_head_tri(&tri_bytes()).expect("valid TRI");
        let surface = build_head_part_surface(&head_part(), &tri, &[("lift", 1.0)])
            .expect("head part renders");

        assert_eq!(surface.positions.len(), 3);
        assert_eq!(surface.triangle_count(), 1);
        assert_eq!(surface.uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(surface.normals.len(), 3, "morphed normals are regenerated");
        assert!(surface.positions.iter().all(|position| position[1] > 0.0));
        assert!(surface.bounds.max.y > 0.0);
    }
}
