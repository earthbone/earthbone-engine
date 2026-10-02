// SPDX-License-Identifier: GPL-3.0-or-later

//! Merges a cell's surfaces per material into one surface each, so the Godot
//! layer adds one `MeshInstance3D` per material set instead of one per NIF
//! shape. The Bannered Mare went from 763 nodes to a few dozen this way, and
//! Godot's frame time follows the object count.
//!
//! Merging into one `ArrayMesh` is the default for two surfaces sharing a
//! material, since the vertex data is already placement baked and there is
//! no per-instance transform to keep. A NIF model placed often enough takes
//! a different path instead: [`instancing_candidates`] pulls those surfaces
//! out before batching and groups them by mesh identity for a `MultiMesh`,
//! one shared geometry buffer plus a transform per placement, which is
//! cheaper than repeating the vertex data once per placement for a model
//! with a heavy buffer and dozens of copies in a cell. `data_source` decides
//! which surfaces qualify (`stream_cell_id` counts placements per model
//! before it builds anything) and keeps their vertex data in the shape's own
//! local space rather than baking the reference placement in, so the
//! `GodotTransform` this module carries per member is the only place that
//! placement lives.
//!
//! Every source surface keeps its place in the merged buffers as a
//! [`BatchMember`], so a triangle index from a raycast maps back to the
//! reference that placed it. A `MultiMesh` group cannot offer that: Godot
//! resolves a hit against its shared geometry to a triangle index, not to
//! which placement drew it, so [`InstanceGroup::reference_of_instance`]
//! answers by placement index instead, and a triangle-based raycast against
//! an instanced group has no member to find. `docs/godot-host.md` lists this
//! as an open item alongside the collision path such picking would need.

use std::collections::HashMap;
use std::sync::Arc;

use crate::data_source::{LoadedSurface, SurfaceTextures};
use crate::mesh::{Aabb, AlphaMode, SurfaceArrays};
use crate::units::GodotTransform;
use soe_esm::GlobalFormId;

/// The fewest placements of one mesh identity (model path plus shape index)
/// that make a `MultiMesh` worth its own draw call and buffer instead of
/// batching those surfaces into a merged `ArrayMesh` with the rest of the
/// cell. Reasoned, not measured: a `MultiMesh` adds a mesh RID, an instance
/// RID and a 48 byte `Transform3D` per placement (12 floats), against a
/// batched copy of the shape's full vertex buffer per placement. A small
/// exterior rock is on the order of a few hundred vertices at roughly 32
/// bytes each (position, normal, tangent, one UV) plus its share of index
/// data, several kilobytes a copy, so even a handful of repeats already cost
/// more merged than instanced. Four is deliberately low: it costs one
/// `MultiMesh` and one draw call to try, and the breaker's benchmark
/// (`docs/godot-host.md` names it) is what tunes this against a measured
/// p99 and memory trace rather than this estimate.
pub const INSTANCE_MIN_PLACEMENTS: usize = 4;

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

/// Maximum number of the largest opaque source surfaces used for one cell's
/// occluder. Sorting uses triangle count and preserves source order on ties.
pub const CELL_OCCLUDER_SURFACES: usize = 4;

/// Flatten the largest opaque source surfaces into one cell-local occluder
/// mesh. MultiMesh candidates are transformed into the same Godot space as
/// their rendered placement. Skinned surfaces move with their skeleton and
/// cannot be stationary occluders. Transparent and cutout surfaces cannot
/// hide geometry behind their holes, so only fully opaque static surfaces
/// qualify.
pub fn cell_occluder_mesh(surfaces: &[LoadedSurface]) -> (Vec<[f32; 3]>, Vec<i32>) {
    let mut candidates: Vec<(usize, usize)> = surfaces
        .iter()
        .enumerate()
        .filter(|(_, surface)| {
            surface.arrays.material.alpha == AlphaMode::Opaque
                && !surface.arrays.is_skinned()
                && surface.arrays.triangle_count() > 0
        })
        .map(|(index, surface)| (index, surface.arrays.triangle_count()))
        .collect();
    candidates.sort_by(|(a_index, a_size), (b_index, b_size)| {
        b_size.cmp(a_size).then_with(|| a_index.cmp(b_index))
    });
    candidates.truncate(CELL_OCCLUDER_SURFACES);

    let vertex_count = candidates
        .iter()
        .map(|(index, _)| surfaces[*index].arrays.positions.len())
        .sum();
    let index_count = candidates
        .iter()
        .map(|(index, _)| surfaces[*index].arrays.indices.len())
        .sum();
    let mut vertices = Vec::with_capacity(vertex_count);
    let mut indices = Vec::with_capacity(index_count);
    for (index, _) in candidates {
        let surface = &surfaces[index];
        let base = vertices.len() as i32;
        if let Some(transform) = surface.instance_transform {
            for p in &surface.arrays.positions {
                vertices.push([
                    transform.basis[0][0] * p[0]
                        + transform.basis[0][1] * p[1]
                        + transform.basis[0][2] * p[2]
                        + transform.origin.x,
                    transform.basis[1][0] * p[0]
                        + transform.basis[1][1] * p[1]
                        + transform.basis[1][2] * p[2]
                        + transform.origin.y,
                    transform.basis[2][0] * p[0]
                        + transform.basis[2][1] * p[1]
                        + transform.basis[2][2] * p[2]
                        + transform.origin.z,
                ]);
            }
        } else {
            vertices.extend_from_slice(&surface.arrays.positions);
        }
        indices.extend(surface.arrays.indices.iter().map(|i| i + base));
    }
    (vertices, indices)
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

/// One placement of an instanced mesh: which reference put it there and
/// where, in Godot space. The geometry itself lives once in the owning
/// [`InstanceGroup`], local to the shape, not baked to this placement.
#[derive(Debug, Clone, Copy)]
pub struct InstanceMember {
    /// Index into the surface list `instancing_candidates` was called with.
    pub surface: usize,
    pub reference: GlobalFormId,
    pub transform: GodotTransform,
}

/// One mesh identity's surfaces, repeated often enough to draw as a
/// `MultiMesh`: one shared geometry buffer and texture set, one transform
/// per placement.
#[derive(Debug, Clone)]
pub struct InstanceGroup {
    /// The model path and shape index the members share, only used to group
    /// them here, the Godot layer does not need it again.
    pub identity: Arc<str>,
    /// Local-space geometry, one shape, shared by every member.
    pub arrays: SurfaceArrays,
    pub textures: SurfaceTextures,
    pub members: Vec<InstanceMember>,
}

impl InstanceGroup {
    /// The reference that placed the `index`th member, in the order they
    /// were pushed, which is the order the Godot layer sets `MultiMesh`
    /// instance transforms in.
    pub fn reference_of_instance(&self, index: usize) -> Option<GlobalFormId> {
        self.members.get(index).map(|m| m.reference)
    }
}

/// What makes two candidate surfaces the same `MultiMesh`: the mesh
/// identity `data_source` assigned (model path plus shape index) and the
/// [`BatchKey`] its material and vertex layout produce. A `MultiMesh` draws
/// its whole instance set with one shared mesh and one material, so two
/// placements of the same model with a different diffuse texture, the base
/// versus a snow variant for example, need two groups the same way
/// `batch_surfaces` already keeps them in two batches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct InstanceKey {
    identity: Arc<str>,
    material: BatchKey,
}

/// Splits `surfaces` into groups worth drawing as a `MultiMesh` and the
/// indices of the remainder, which still goes through [`batch_surfaces`].
///
/// Only surfaces `data_source` already marked with both a `mesh_identity`
/// and an `instance_transform` are candidates. `data_source` sets both only
/// when it decided, before building any geometry, that the model clears
/// [`INSTANCE_MIN_PLACEMENTS`] in that cell, so this function counts
/// nothing itself, it groups what is already marked. Within that, members
/// are grouped by [`InstanceKey`], mesh identity plus material, since one
/// `MultiMesh` has exactly one material for every instance it draws.
/// Skinned surfaces never become instancing candidates: a `MultiMesh`
/// instance carries only a rigid `Transform3D`, with no per-instance
/// skeleton pose for their bone indices to bind to, the same reason
/// `batch_surfaces` keeps skinned surfaces out of a merged `ArrayMesh`. A
/// candidate whose key turns out to have fewer than `INSTANCE_MIN_PLACEMENTS`
/// members once grouped (two different shapes or materials of the same
/// model can carry the same count from `data_source` but split across keys)
/// falls back to the remainder, since a `MultiMesh` of one or two instances
/// has no case for its own draw call.
pub fn instancing_candidates(surfaces: &[LoadedSurface]) -> (Vec<InstanceGroup>, Vec<usize>) {
    let mut by_key: HashMap<InstanceKey, usize> = HashMap::new();
    let mut groups: Vec<InstanceGroup> = Vec::new();
    let mut remainder: Vec<usize> = Vec::new();
    for (index, surface) in surfaces.iter().enumerate() {
        let (Some(identity), Some(transform)) =
            (surface.mesh_identity.as_ref(), surface.instance_transform)
        else {
            remainder.push(index);
            continue;
        };
        if surface.arrays.is_skinned() {
            remainder.push(index);
            continue;
        }
        let key = InstanceKey {
            identity: Arc::clone(identity),
            material: BatchKey::of(&surface.arrays),
        };
        let member = InstanceMember {
            surface: index,
            reference: surface.reference,
            transform,
        };
        match by_key.get(&key) {
            Some(&slot) => groups[slot].members.push(member),
            None => {
                by_key.insert(key, groups.len());
                groups.push(InstanceGroup {
                    identity: Arc::clone(identity),
                    arrays: surface.arrays.clone(),
                    textures: surface.textures.clone(),
                    members: vec![member],
                });
            }
        }
    }
    let (groups, short): (Vec<_>, Vec<_>) = groups
        .into_iter()
        .partition(|g| g.members.len() >= INSTANCE_MIN_PLACEMENTS);
    remainder.extend(
        short
            .iter()
            .flat_map(|g| g.members.iter().map(|m| m.surface)),
    );
    remainder.sort_unstable();
    (groups, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions};

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
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        }
    }

    fn repeated_triangle(offset: f32, triangles: usize) -> LoadedSurface {
        let data = MeshData {
            positions: vec![
                [offset, 0.0, 0.0],
                [offset + 1.0, 0.0, 0.0],
                [offset, 1.0, 0.0],
            ],
            indices: (0..triangles).flat_map(|_| [0, 1, 2]).collect(),
            ..MeshData::default()
        };
        LoadedSurface {
            reference: GlobalFormId {
                file: 0,
                object: offset as u32,
            },
            arrays: build_surface(&data, MeshOptions::default()).unwrap(),
            textures: SurfaceTextures::default(),
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        }
    }

    #[test]
    fn rnd_07_cell_occluder_uses_the_largest_opaque_surfaces() {
        let mut surfaces: Vec<_> = (1..=6)
            .map(|triangles| repeated_triangle(triangles as f32 * 10.0, triangles))
            .collect();
        surfaces[5].arrays.material.alpha = AlphaMode::Cutout(128);

        let (vertices, indices) = cell_occluder_mesh(&surfaces);

        assert_eq!(vertices.len(), 12);
        assert_eq!(indices.len(), (5 + 4 + 3 + 2) * 3);
        let offsets: Vec<_> = vertices.iter().step_by(3).map(|p| p[0]).collect();
        for (actual, units) in offsets.iter().zip([50.0, 40.0, 30.0, 20.0]) {
            assert!((actual - units * crate::units::METRES_PER_UNIT).abs() < 1e-6);
        }
    }

    #[test]
    fn rnd_07_cell_occluder_applies_multimesh_placement() {
        let mut surface = repeated_triangle(0.0, 1);
        surface.instance_transform = Some(GodotTransform {
            basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            origin: crate::units::GodotPos::new(7.0, 8.0, 9.0),
        });

        let (vertices, _) = cell_occluder_mesh(&[surface]);

        let expected = [
            [7.0, 8.0, 9.0],
            [7.0 + crate::units::METRES_PER_UNIT, 8.0, 9.0],
            [7.0, 8.0, 9.0 - crate::units::METRES_PER_UNIT],
        ];
        for (actual, expected) in vertices.iter().zip(expected) {
            for (actual, expected) in actual.iter().zip(expected) {
                assert!((actual - expected).abs() < 1e-6);
            }
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
