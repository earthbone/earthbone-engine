// SPDX-License-Identifier: GPL-3.0-or-later

//! Merges a cell's surfaces per material into one surface each, so the Godot
//! layer adds one `MeshInstance3D` per material set instead of one per NIF
//! shape. The Bannered Mare went from 763 nodes to a few dozen this way, and
//! Godot's frame time follows the object count.
//!
//! Merging into one `ArrayMesh` was chosen over a `MultiMesh` per repeated
//! NIF because `data_source::mesh_data_from` has already baked each
//! reference's placement into the vertex data, so a `MultiMesh` would need
//! that transform undone and re-applied per instance, and because the two
//! surfaces of a cell that share a NIF share its material too, so they land
//! in the same batch either way. A `MultiMesh` pays off where one NIF is
//! placed hundreds of times with a heavy vertex buffer, which is the
//! exterior rock and tree case, and is the next step once that shows in a
//! trace.
//!
//! Every source surface keeps its place in the merged buffers as a
//! [`BatchMember`], so a triangle index from a raycast maps back to the
//! reference that placed it.

use std::collections::HashMap;

use crate::data_source::{LoadedSurface, SurfaceTextures};
use crate::mesh::{Aabb, AlphaMode, SurfaceArrays};

/// What has to agree for two surfaces to share one draw: the texture set,
/// the alpha mode, the cull mode, and which vertex attributes are present,
/// since a merged vertex buffer has one layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BatchKey {
    pub environment: Option<String>,
    pub environment_mask: Option<String>,
    pub environment_scale: u32,
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub glow: Option<String>,
    pub emissive_color: [u32; 3],
    pub emissive_multiple: u32,
    /// `AlphaMode` flattened to a kind byte and a cutout threshold.
    pub alpha: (u8, u8),
    pub double_sided: bool,
    pub decal_depth: Option<(bool, bool)>,
    pub has_normals: bool,
    pub has_tangents: bool,
    pub has_uvs: bool,
}

impl BatchKey {
    pub fn of(surface: &SurfaceArrays) -> BatchKey {
        let m = &surface.material;
        BatchKey {
            environment: m.environment_texture().map(str::to_lowercase),
            environment_mask: m.environment_mask.as_deref().map(str::to_lowercase),
            environment_scale: m.environment_scale.to_bits(),
            diffuse: m.diffuse.as_deref().map(str::to_lowercase),
            normal: m.normal.as_deref().map(str::to_lowercase),
            glow: m.glow_texture().map(str::to_lowercase),
            emissive_color: m.emissive_color.map(f32::to_bits),
            emissive_multiple: m.emissive_multiple.to_bits(),
            alpha: match m.alpha {
                AlphaMode::Opaque => (0, 0),
                AlphaMode::Cutout(t) => (1, t),
                AlphaMode::Blend => (2, 0),
            },
            double_sided: m.double_sided,
            decal_depth: m.decal_depth(),
            has_normals: !surface.normals.is_empty(),
            has_tangents: !surface.tangents.is_empty(),
            has_uvs: !surface.uvs.is_empty(),
        }
    }
}

/// Where one source surface landed inside a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchMember {
    /// Index into the surface list the batch was built from.
    pub surface: usize,
    pub first_vertex: usize,
    pub vertex_count: usize,
    pub first_triangle: usize,
    pub triangle_count: usize,
}

/// One merged surface and the members it was built from, in order.
#[derive(Debug, Clone)]
pub struct Batch {
    pub key: BatchKey,
    pub arrays: SurfaceArrays,
    /// The first member's textures. Every member shares the texture set by
    /// construction of the key.
    pub textures: SurfaceTextures,
    pub members: Vec<BatchMember>,
}

impl Batch {
    /// The member owning `triangle`, an index into the merged triangle list.
    pub fn member_of_triangle(&self, triangle: usize) -> Option<&BatchMember> {
        let at = self
            .members
            .partition_point(|m| m.first_triangle + m.triangle_count <= triangle);
        self.members.get(at).filter(|m| {
            triangle >= m.first_triangle && triangle < m.first_triangle + m.triangle_count
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchStats {
    pub surfaces_in: usize,
    pub batches: usize,
    pub largest_batch_surfaces: usize,
    pub triangles: usize,
}

/// Merge `surfaces` by [`BatchKey`], first-seen order. Skinned surfaces are
/// never merged, each becomes its own batch, since their bone indices refer
/// to their own skeleton.
pub fn batch_surfaces(surfaces: &[LoadedSurface]) -> (Vec<Batch>, BatchStats) {
    let _span = soe_trace::Span::with_payload("host.batch", "host", surfaces.len() as u64);
    let mut batches: Vec<Batch> = Vec::new();
    let mut by_key: HashMap<BatchKey, usize> = HashMap::new();
    for (index, surface) in surfaces.iter().enumerate() {
        let key = BatchKey::of(&surface.arrays);
        let slot = if surface.arrays.is_skinned() {
            None
        } else {
            by_key.get(&key).copied()
        };
        match slot {
            Some(slot) => append(&mut batches[slot], index, &surface.arrays),
            None => {
                if !surface.arrays.is_skinned() {
                    by_key.insert(key.clone(), batches.len());
                }
                batches.push(Batch {
                    key,
                    arrays: surface.arrays.clone(),
                    textures: surface.textures.clone(),
                    members: vec![BatchMember {
                        surface: index,
                        first_vertex: 0,
                        vertex_count: surface.arrays.positions.len(),
                        first_triangle: 0,
                        triangle_count: surface.arrays.triangle_count(),
                    }],
                });
            }
        }
    }
    let stats = BatchStats {
        surfaces_in: surfaces.len(),
        batches: batches.len(),
        largest_batch_surfaces: batches.iter().map(|b| b.members.len()).max().unwrap_or(0),
        triangles: batches.iter().map(|b| b.arrays.triangle_count()).sum(),
    };
    (batches, stats)
}

fn append(batch: &mut Batch, index: usize, src: &SurfaceArrays) {
    let dst = &mut batch.arrays;
    let base = dst.positions.len();
    batch.members.push(BatchMember {
        surface: index,
        first_vertex: base,
        vertex_count: src.positions.len(),
        first_triangle: dst.triangle_count(),
        triangle_count: src.triangle_count(),
    });
    dst.positions.extend_from_slice(&src.positions);
    dst.normals.extend_from_slice(&src.normals);
    dst.tangents.extend_from_slice(&src.tangents);
    dst.uvs.extend_from_slice(&src.uvs);
    dst.indices
        .extend(src.indices.iter().map(|&i| i + base as i32));
    dst.bounds = merge_bounds(dst.bounds, src.bounds);
}

/// The smallest box holding both.
pub fn merge_bounds(a: Aabb, b: Aabb) -> Aabb {
    use crate::units::GodotPos;
    Aabb {
        min: GodotPos::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
        ),
        max: GodotPos::new(
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions};
    use soe_esm::GlobalFormId;

    fn quad(offset: f32, diffuse: &str) -> LoadedSurface {
        let data = MeshData {
            positions: vec![
                [offset, 0.0, 0.0],
                [offset + 70.0, 0.0, 0.0],
                [offset, 70.0, 0.0],
                [offset + 70.0, 70.0, 0.0],
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            uvs: Some(vec![[0.0, 0.0]; 4]),
            material: MaterialDesc {
                diffuse: Some(diffuse.to_string()),
                ..MaterialDesc::default()
            },
            ..MeshData::default()
        };
        LoadedSurface {
            reference: GlobalFormId {
                file: 0,
                object: offset as u32,
            },
            arrays: build_surface(&data, MeshOptions::default()).unwrap(),
            textures: SurfaceTextures::default(),
        }
    }

    #[test]
    fn surfaces_sharing_a_material_merge_with_offset_indices() {
        let surfaces = vec![
            quad(0.0, "textures\\a.dds"),
            quad(100.0, "TEXTURES\\A.DDS"),
            quad(200.0, "textures\\b.dds"),
        ];
        let (batches, stats) = batch_surfaces(&surfaces);
        assert_eq!(stats.batches, 2);
        assert_eq!(stats.surfaces_in, 3);
        assert_eq!(stats.largest_batch_surfaces, 2);
        assert_eq!(stats.triangles, 6);

        let a = &batches[0];
        assert_eq!(a.arrays.positions.len(), 8);
        assert_eq!(a.arrays.uvs.len(), 8);
        assert_eq!(a.arrays.normals.len(), 8);
        assert_eq!(a.arrays.triangle_count(), 4);
        assert_eq!(&a.arrays.indices[6..12], &[4, 6, 5, 6, 7, 5]);
        assert_eq!(a.members[1].first_vertex, 4);
        assert_eq!(a.members[1].first_triangle, 2);
        assert!(a.arrays.bounds.max.x > surfaces[1].arrays.bounds.min.x);
        assert_eq!(a.arrays.bounds.min.x, 0.0);

        assert_eq!(a.member_of_triangle(0).map(|m| m.surface), Some(0));
        assert_eq!(a.member_of_triangle(3).map(|m| m.surface), Some(1));
        assert!(a.member_of_triangle(4).is_none());
        assert_eq!(batches[1].members[0].surface, 2);
    }

    #[test]
    fn attribute_layout_and_alpha_split_batches() {
        let mut with_normals = quad(0.0, "t.dds");
        with_normals.arrays.normals.clear();
        let cutout = {
            let mut s = quad(0.0, "t.dds");
            s.arrays.material.alpha = AlphaMode::Cutout(128);
            s
        };
        let plain = quad(0.0, "t.dds");
        let (batches, _) = batch_surfaces(&[with_normals, cutout, plain]);
        assert_eq!(batches.len(), 3);
    }

    #[test]
    fn skinned_surfaces_stay_alone() {
        let mut a = quad(0.0, "t.dds");
        a.arrays.bones = vec![0; 16];
        a.arrays.weights = vec![1.0; 16];
        let b = a.clone();
        let (batches, _) = batch_surfaces(&[a, b]);
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn decals_and_depth_variants_do_not_merge_with_their_base() {
        use soe_nif::blocks::{flags1, flags2};
        let plain = quad(0.0, "t.dds");
        let mut surfaces = vec![plain.clone()];
        for test in [0, flags1::ZBUFFER_TEST] {
            for write in [0, flags2::ZBUFFER_WRITE] {
                let mut decal = plain.clone();
                decal.arrays.material.shader_flags_1 = flags1::DECAL | test;
                decal.arrays.material.shader_flags_2 = write;
                surfaces.push(decal);
            }
        }
        let (batches, _) = batch_surfaces(&surfaces);
        assert_eq!(batches.len(), 5);
    }
}
