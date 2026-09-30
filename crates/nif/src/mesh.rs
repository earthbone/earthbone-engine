// SPDX-License-Identifier: GPL-3.0-or-later

//! The renderer-neutral output of this crate. A host engine (the Godot host
//! mirrors these structs) consumes `Mesh` and never has to know which NIF
//! block family a shape came from.
//!
//! Conventions, all inherited from the file:
//! - Space is right-handed, Z up, one unit is one game unit.
//! - `transform` is the shape's world transform accumulated through its
//!   parent nodes from the file's root. Vertex data is in the shape's local
//!   space, so a host applies `transform` (or parents the mesh under a node
//!   that carries it).
//! - Triangles are counter-clockwise front faces as Gamebryo stores them.
//! - UV origin is top-left, the DirectX convention, so a host that expects
//!   OpenGL flips `v`.
//! - A skinned mesh's positions are in the skin's bind space, and each
//!   `Bone` carries the inverse bind transform (the NIF "skin transform")
//!   to move a vertex into that bone's space.

use crate::blocks::{
    flags1, flags2, AlphaProperty, BlendFactor, Block, BodyPart, BsTriShape, EffectShader,
    GeometryData, LightingShader, ShaderExtra, ShaderType, SkinInstance, TestFunc, TriShape,
};
use crate::error::{Error, Result};
use crate::types::{BlockRef, Bound, Transform, Vec3};
use crate::vertex::{attr, Vertex};
use crate::NifFile;

#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// The shape block's name, for example `WhiterunDoor:0`.
    pub name: String,
    /// Index of the shape block in the file.
    pub block: usize,
    /// World transform accumulated from the root node.
    pub transform: Transform,
    pub bound: Bound,
    pub positions: Vec<[f32; 3]>,
    /// Empty when the shape has no normals.
    pub normals: Vec<[f32; 3]>,
    /// `[x, y, z, w]` with `w` the bitangent handedness (1 or -1). Empty
    /// when the shape has no tangents.
    pub tangents: Vec<[f32; 4]>,
    /// First UV set, retained for callers that only use one set. Empty when
    /// the shape has none.
    pub uvs: Vec<[f32; 2]>,
    /// One list per UV set, each with one coordinate per vertex.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    /// RGBA in 0 to 1. Empty when the shape has no vertex colors.
    pub colors: Vec<[f32; 4]>,
    /// Triangle list, three indices per triangle.
    pub indices: Vec<u32>,
    /// Four weights per vertex, empty when unskinned. Indices refer to
    /// `skin.bones`.
    pub bone_weights: Vec<[f32; 4]>,
    pub bone_indices: Vec<[u8; 4]>,
    pub skin: Option<Skin>,
    pub material: Material,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Rough resident size in bytes: each present vertex attribute at its
    /// natural width, plus the index buffer at 4 bytes an index. Normals
    /// are charged whether or not this shape carries them, since a host
    /// with `generate_missing_normals` on (the default) builds a full
    /// normals buffer from a shape with none, the ordinary case for a
    /// space-saving distant LOD mesh, and a residency estimate taken before
    /// that generation must not read low against what actually gets
    /// uploaded. Vertex colors are never charged: nothing downstream of
    /// this struct keeps a color buffer past the decoded shape (the host's
    /// mesh source and built surface carry no color field), so counting
    /// them would charge for bytes nothing ever allocates. Close enough to
    /// weigh a mesh against a residency budget, not a byte-exact account of
    /// any one renderer's buffer layout.
    pub fn approx_bytes(&self) -> u64 {
        let per_vertex = 12 // positions
            + 12 // normals, present or generated downstream
            + if self.tangents.is_empty() { 0 } else { 16 }
            + self.uv_sets.len().max(usize::from(!self.uvs.is_empty())) as u64 * 8
            + if self.bone_weights.is_empty() { 0 } else { 16 }
            + if self.bone_indices.is_empty() { 0 } else { 4 };
        self.positions.len() as u64 * per_vertex + self.indices.len() as u64 * 4
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub skeleton_root: BlockRef,
    pub skeleton_root_name: String,
    /// The skin's own offset from the skeleton root.
    pub transform: Transform,
    pub bones: Vec<Bone>,
    /// BSDismemberSkinInstance body part tags, one per partition.
    pub body_parts: Vec<BodyPart>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub node: BlockRef,
    /// The NIF skin transform: bind-space vertex to bone space.
    pub inverse_bind: Transform,
    pub bound: Bound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialKind {
    /// No shader property on the shape.
    None,
    Lighting(ShaderType),
    Effect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blend {
    pub src: BlendFactor,
    pub dst: BlendFactor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaTest {
    pub func: TestFunc,
    /// 0 to 1, from the byte threshold.
    pub threshold: f32,
}

/// Effect shader parameters, present when `kind` is `Effect`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectParams {
    pub source_texture: String,
    pub greyscale_texture: String,
    pub base_color: [f32; 4],
    pub base_color_scale: f32,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    pub soft_falloff_depth: f32,
    pub lighting_influence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub kind: MaterialKind,
    /// Texture paths as written, relative to `Data\textures` with
    /// backslashes. Slot meaning is fixed by Skyrim, see the accessors.
    pub textures: Vec<String>,
    pub flags1: u32,
    pub flags2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub emissive_color: [f32; 3],
    pub emissive_multiple: f32,
    pub alpha: f32,
    /// Specular power.
    pub glossiness: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    pub double_sided: bool,
    pub vertex_colors: bool,
    pub vertex_alpha: bool,
    pub model_space_normals: bool,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
    pub zbuffer_write: bool,
    pub zbuffer_test: bool,
    pub decal: bool,
    pub environment_mapping: bool,
    pub blend: Option<Blend>,
    pub alpha_test: Option<AlphaTest>,
    pub extra: ShaderExtra,
    pub effect: Option<EffectParams>,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            kind: MaterialKind::None,
            textures: Vec::new(),
            flags1: 0,
            flags2: 0,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            emissive_color: [0.0; 3],
            emissive_multiple: 1.0,
            alpha: 1.0,
            glossiness: 80.0,
            specular_color: [1.0; 3],
            specular_strength: 1.0,
            double_sided: false,
            vertex_colors: false,
            vertex_alpha: false,
            model_space_normals: false,
            cast_shadows: true,
            receive_shadows: true,
            zbuffer_write: true,
            zbuffer_test: true,
            decal: false,
            environment_mapping: false,
            blend: None,
            alpha_test: None,
            extra: ShaderExtra::None,
            effect: None,
        }
    }
}

impl Material {
    fn slot(&self, i: usize) -> Option<&str> {
        self.textures
            .get(i)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    pub fn diffuse(&self) -> Option<&str> {
        self.slot(0)
    }

    /// Normal map, with specular in its alpha channel.
    pub fn normal(&self) -> Option<&str> {
        self.slot(1)
    }

    /// Glow map, or the subsurface map for skin and hair shaders.
    pub fn glow(&self) -> Option<&str> {
        self.slot(2)
    }

    pub fn height(&self) -> Option<&str> {
        self.slot(3)
    }

    pub fn environment(&self) -> Option<&str> {
        self.slot(4)
    }

    pub fn environment_mask(&self) -> Option<&str> {
        self.slot(5)
    }

    /// Multilayer parallax inner layer, or the face tint mask.
    pub fn inner_layer(&self) -> Option<&str> {
        self.slot(6)
    }

    /// Backlight or specular map.
    pub fn backlight(&self) -> Option<&str> {
        self.slot(7)
    }

    fn apply_flags(&mut self) {
        self.double_sided = self.flags2 & flags2::DOUBLE_SIDED != 0;
        self.vertex_colors = self.flags2 & flags2::VERTEX_COLORS != 0;
        self.vertex_alpha = self.flags1 & flags1::VERTEX_ALPHA != 0;
        self.model_space_normals = self.flags1 & flags1::MODEL_SPACE_NORMALS != 0;
        self.cast_shadows = self.flags1 & flags1::CAST_SHADOWS != 0;
        self.receive_shadows = self.flags1 & flags1::RECEIVE_SHADOWS != 0;
        self.zbuffer_write = self.flags2 & flags2::ZBUFFER_WRITE != 0;
        self.zbuffer_test = self.flags1 & flags1::ZBUFFER_TEST != 0;
        self.decal = self.flags1 & (flags1::DECAL | flags1::DYNAMIC_DECAL) != 0;
        self.environment_mapping = self.flags1 & flags1::ENVIRONMENT_MAPPING != 0;
    }

    fn from_lighting(p: &LightingShader, textures: Vec<String>) -> Material {
        let mut m = Material {
            kind: MaterialKind::Lighting(p.shader_type),
            textures,
            flags1: p.flags1,
            flags2: p.flags2,
            uv_offset: p.uv_offset,
            uv_scale: p.uv_scale,
            emissive_color: p.emissive_color,
            emissive_multiple: p.emissive_multiple,
            alpha: p.alpha,
            glossiness: p.glossiness,
            specular_color: p.specular_color,
            specular_strength: p.specular_strength,
            extra: p.extra.clone(),
            ..Default::default()
        };
        m.apply_flags();
        m
    }

    fn from_effect(p: &EffectShader) -> Material {
        let mut m = Material {
            kind: MaterialKind::Effect,
            textures: vec![p.source_texture.clone()],
            flags1: p.flags1,
            flags2: p.flags2,
            uv_offset: p.uv_offset,
            uv_scale: p.uv_scale,
            effect: Some(EffectParams {
                source_texture: p.source_texture.clone(),
                greyscale_texture: p.greyscale_texture.clone(),
                base_color: p.base_color,
                base_color_scale: p.base_color_scale,
                falloff_start_angle: p.falloff_start_angle,
                falloff_stop_angle: p.falloff_stop_angle,
                falloff_start_opacity: p.falloff_start_opacity,
                falloff_stop_opacity: p.falloff_stop_opacity,
                soft_falloff_depth: p.soft_falloff_depth,
                lighting_influence: p.lighting_influence as f32 / 255.0,
            }),
            ..Default::default()
        };
        m.apply_flags();
        m
    }

    fn apply_alpha(&mut self, a: &AlphaProperty) {
        if a.blend_enabled() {
            self.blend = Some(Blend {
                src: a.src_blend(),
                dst: a.dst_blend(),
            });
        }
        if a.test_enabled() {
            self.alpha_test = Some(AlphaTest {
                func: a.test_func(),
                threshold: a.threshold as f32 / 255.0,
            });
        }
    }
}

/// Per-vertex bone weights and the matching bone indices.
type VertexWeights = (Vec<[f32; 4]>, Vec<[u8; 4]>);

/// Tangent handedness from the stored bitangent: positive when the stored
/// bitangent agrees with `normal x tangent`.
fn handedness(n: Vec3, t: Vec3, b: Vec3) -> f32 {
    if n.cross(t).dot(b) < 0.0 {
        -1.0
    } else {
        1.0
    }
}

struct Walker<'f> {
    file: &'f NifFile,
    out: Vec<Mesh>,
    stack: Vec<usize>,
}

impl<'f> Walker<'f> {
    fn block(&self, from: usize, r: BlockRef) -> Result<Option<&'f Block>> {
        match r.get() {
            None => Ok(None),
            Some(i) => self.file.blocks.get(i).map(Some).ok_or(Error::BadRef {
                block: from,
                reference: r.0,
            }),
        }
    }

    fn walk(&mut self, r: BlockRef, parent: &Transform) -> Result<()> {
        let Some(idx) = r.get() else {
            return Ok(());
        };
        let Some(block) = self.file.blocks.get(idx) else {
            return Err(Error::BadRef {
                block: self.stack.last().copied().unwrap_or(0),
                reference: r.0,
            });
        };
        if self.stack.contains(&idx) || self.stack.len() > 256 {
            return Err(Error::Cycle { block: idx });
        }
        self.stack.push(idx);
        match block {
            Block::Node(n) => {
                let t = parent.then(&n.av.transform);
                for &child in &n.children {
                    self.walk(child, &t)?;
                }
            }
            Block::BsTriShape(t) => {
                let mesh = self.bs_tri_shape(idx, t, parent)?;
                self.out.push(mesh);
            }
            Block::TriShape(t) => {
                if let Some(mesh) = self.tri_shape(idx, t, parent)? {
                    self.out.push(mesh);
                }
            }
            _ => {}
        }
        self.stack.pop();
        Ok(())
    }

    fn material(&self, idx: usize, shader: BlockRef, alpha: BlockRef) -> Result<Material> {
        let mut m = match self.block(idx, shader)? {
            Some(Block::LightingShader(p)) => {
                let textures = match self.block(idx, p.texture_set)? {
                    Some(Block::TextureSet(t)) => t.textures.clone(),
                    _ => Vec::new(),
                };
                Material::from_lighting(p, textures)
            }
            Some(Block::EffectShader(p)) => Material::from_effect(p),
            _ => Material::default(),
        };
        if let Some(Block::AlphaProperty(a)) = self.block(idx, alpha)? {
            m.apply_alpha(a);
        }
        Ok(m)
    }

    fn skin(&self, idx: usize, skin: BlockRef) -> Result<Option<(&'f SkinInstance, Skin)>> {
        let Some(Block::SkinInstance(si)) = self.block(idx, skin)? else {
            return Ok(None);
        };
        let data = match self.block(idx, si.data)? {
            Some(Block::SkinData(d)) => Some(d),
            _ => None,
        };
        let mut bones = Vec::with_capacity(si.bones.len());
        for (i, &b) in si.bones.iter().enumerate() {
            let name = self.file.node_name(b).unwrap_or_default();
            let (inverse_bind, bound) = data
                .and_then(|d| d.bones.get(i))
                .map(|sb| (sb.transform, sb.bound))
                .unwrap_or_default();
            bones.push(Bone {
                name,
                node: b,
                inverse_bind,
                bound,
            });
        }
        Ok(Some((
            si,
            Skin {
                skeleton_root: si.skeleton_root,
                skeleton_root_name: self.file.node_name(si.skeleton_root).unwrap_or_default(),
                transform: data.map(|d| d.transform).unwrap_or_default(),
                bones,
                body_parts: si.body_parts.clone(),
            },
        )))
    }

    fn bs_tri_shape(&self, idx: usize, t: &BsTriShape, parent: &Transform) -> Result<Mesh> {
        let material = self.material(idx, t.shader, t.alpha)?;
        let skin = self.skin(idx, t.skin)?;
        // A skinned SSE shape keeps its vertices in the skin partition.
        let partition = skin
            .as_ref()
            .and_then(|(si, _)| match self.block(idx, si.partition) {
                Ok(Some(Block::SkinPartition(p))) => Some(p),
                _ => None,
            });
        let (vertices, attrs, triangles): (&[Vertex], u16, Vec<[u16; 3]>) = match partition {
            Some(p) if t.vertices.is_empty() => {
                let mut tris = Vec::new();
                for part in &p.partitions {
                    if part.triangles_copy.is_empty() {
                        for tri in &part.triangles {
                            let mapped =
                                tri.map(|v| part.vertex_map.get(v as usize).copied().unwrap_or(v));
                            tris.push(mapped);
                        }
                    } else {
                        tris.extend_from_slice(&part.triangles_copy);
                    }
                }
                (&p.vertices, p.desc.attributes(), tris)
            }
            _ => (&t.vertices, t.desc.attributes(), t.triangles.clone()),
        };
        let n = vertices.len();
        for (i, tri) in triangles.iter().enumerate() {
            if tri.iter().any(|&v| v as usize >= n) {
                return Err(Error::InvalidData {
                    block: idx,
                    reason: format!("skin partition triangle {i} indexes past the {n} vertices"),
                });
            }
        }
        let mut mesh = Mesh {
            name: t.av.name().to_string(),
            block: idx,
            transform: parent.then(&t.av.transform),
            bound: t.bound,
            positions: vertices.iter().map(|v| v.position.to_array()).collect(),
            normals: Vec::new(),
            tangents: Vec::new(),
            uvs: Vec::new(),
            uv_sets: Vec::new(),
            colors: Vec::new(),
            indices: triangles.iter().flat_map(|t| t.map(u32::from)).collect(),
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
            skin: skin.map(|(_, s)| s),
            material,
        };
        if attrs & attr::NORMALS != 0 {
            mesh.normals = vertices.iter().map(|v| v.normal.to_array()).collect();
            if attrs & attr::TANGENTS != 0 {
                mesh.tangents = vertices
                    .iter()
                    .map(|v| {
                        let w = handedness(v.normal, v.tangent, v.bitangent);
                        [v.tangent.x, v.tangent.y, v.tangent.z, w]
                    })
                    .collect();
            }
        }
        if attrs & attr::UVS != 0 {
            mesh.uvs = vertices.iter().map(|v| v.uv).collect();
            mesh.uv_sets.push(mesh.uvs.clone());
        }
        if attrs & attr::COLORS != 0 {
            mesh.colors = vertices.iter().map(|v| v.color).collect();
        }
        if attrs & attr::SKINNED != 0 && mesh.skin.is_some() {
            mesh.bone_weights = vertices.iter().map(|v| v.bone_weights).collect();
            mesh.bone_indices = vertices.iter().map(|v| v.bone_indices).collect();
            // A bone index stored inside a partition addresses that
            // partition's own short bone list, not the skin instance's, and
            // `Mesh::bone_indices` is documented as the latter. The legacy
            // path has always mapped them through `part.bones`, this one
            // copied them raw until 2026-09-18.
            // Only when the vertices came from the partition. A shape that
            // keeps its own vertex array carries skin-instance indices
            // already, and remapping those would corrupt them.
            let from_partition = partition.filter(|_| t.vertices.is_empty());
            if let Some(p) = from_partition {
                for part in &p.partitions {
                    if part.bones.is_empty() {
                        continue;
                    }
                    for &global in &part.vertex_map {
                        let Some(slot) = mesh.bone_indices.get_mut(global as usize) else {
                            continue;
                        };
                        for b in slot.iter_mut() {
                            let mapped = part.bones.get(*b as usize).copied().unwrap_or(0);
                            *b = u8::try_from(mapped).unwrap_or(0);
                        }
                    }
                }
            }
        }
        Ok(mesh)
    }

    fn tri_shape(&self, idx: usize, t: &TriShape, parent: &Transform) -> Result<Option<Mesh>> {
        let Some(Block::GeometryData(g)) = self.block(idx, t.data)? else {
            return Ok(None);
        };
        let material = self.material(idx, t.shader, t.alpha)?;
        let skin = self.skin(idx, t.skin)?;
        let mut mesh = Mesh {
            name: t.av.name().to_string(),
            block: idx,
            transform: parent.then(&t.av.transform),
            bound: g.bound,
            positions: g.vertices.iter().map(|v| v.to_array()).collect(),
            normals: g.normals.iter().map(|v| v.to_array()).collect(),
            tangents: Vec::new(),
            uvs: g.uv_sets.first().cloned().unwrap_or_default(),
            uv_sets: g.uv_sets.clone(),
            colors: g.colors.clone(),
            indices: g.triangles.iter().flat_map(|t| t.map(u32::from)).collect(),
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
            skin: None,
            material,
        };
        if !g.tangents.is_empty() && g.tangents.len() == g.normals.len() {
            mesh.tangents = (0..g.tangents.len())
                .map(|i| {
                    let (n, t) = (g.normals[i], g.tangents[i]);
                    let b = g.bitangents.get(i).copied().unwrap_or(n.cross(t));
                    [t.x, t.y, t.z, handedness(n, t, b)]
                })
                .collect();
        }
        if let Some((si, s)) = skin {
            let (weights, indices) = self.legacy_weights(idx, si, g)?;
            mesh.bone_weights = weights;
            mesh.bone_indices = indices;
            mesh.skin = Some(s);
        }
        Ok(Some(mesh))
    }

    /// Per-vertex weights for a NiTriShape: from the partition when it has
    /// them (bone indices there are partition-local and mapped to the skin
    /// instance's bone list), otherwise from NiSkinData's per-bone lists.
    fn legacy_weights(
        &self,
        idx: usize,
        si: &SkinInstance,
        g: &GeometryData,
    ) -> Result<VertexWeights> {
        let n = g.vertices.len();
        let mut weights = vec![[0.0f32; 4]; n];
        let mut indices = vec![[0u8; 4]; n];
        let mut filled = vec![0usize; n];
        let mut push = |v: usize, bone: usize, w: f32| {
            if v >= n || w == 0.0 {
                return;
            }
            let slot = filled[v];
            if slot < 4 {
                weights[v][slot] = w;
                indices[v][slot] = bone.min(255) as u8;
                filled[v] += 1;
            }
        };
        if let Some(Block::SkinPartition(p)) = self.block(idx, si.partition)? {
            let mut any = false;
            for part in &p.partitions {
                let bpv = part.bones_per_vertex as usize;
                if part.weights.is_empty() || part.bone_indices.is_empty() || bpv == 0 {
                    continue;
                }
                any = true;
                for (local, &global) in part.vertex_map.iter().enumerate() {
                    for k in 0..bpv {
                        let w = part.weights.get(local * bpv + k).copied().unwrap_or(0.0);
                        let bi = part.bone_indices.get(local * bpv + k).copied().unwrap_or(0);
                        let bone = part.bones.get(bi as usize).copied().unwrap_or(0) as usize;
                        push(global as usize, bone, w);
                    }
                }
            }
            if any {
                return Ok((weights, indices));
            }
        }
        if let Some(Block::SkinData(d)) = self.block(idx, si.data)? {
            for (bone, sb) in d.bones.iter().enumerate() {
                for &(v, w) in &sb.weights {
                    push(v as usize, bone, w);
                }
            }
        }
        Ok((weights, indices))
    }
}

/// Every mesh reachable from the file's roots, in depth-first order.
pub fn meshes(file: &NifFile) -> Result<Vec<Mesh>> {
    let mut w = Walker {
        file,
        out: Vec::new(),
        stack: Vec::new(),
    };
    let roots: Vec<BlockRef> = if file.roots.is_empty() && !file.blocks.is_empty() {
        vec![BlockRef(0)]
    } else {
        file.roots.clone()
    };
    for r in roots {
        w.walk(r, &Transform::IDENTITY)?;
    }
    Ok(w.out)
}

/// Meshes under `node`'s children, walked from the identity transform
/// instead of `node`'s own transform.
///
/// A host driving `node` live through a [`crate::controller`] transform
/// controller wants its child geometry in `node`'s own local space, not
/// baked with `node`'s default pose, so it can apply the evaluated
/// transform itself every frame instead of double counting it. `meshes`
/// bakes every ancestor including `node`, which is right for a static shape
/// but wrong for one this function's caller repositions each frame.
pub fn meshes_under_children(file: &NifFile, node: BlockRef) -> Result<Vec<Mesh>> {
    let Some(idx) = node.get() else {
        return Ok(Vec::new());
    };
    let Some(Block::Node(n)) = file.blocks.get(idx) else {
        return Err(Error::InvalidData {
            block: idx,
            reason: "meshes_under_children: block is not an NiNode".into(),
        });
    };
    let mut w = Walker {
        file,
        out: Vec::new(),
        stack: vec![idx],
    };
    for &child in &n.children {
        w.walk(child, &Transform::IDENTITY)?;
    }
    Ok(w.out)
}
