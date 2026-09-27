// SPDX-License-Identifier: GPL-3.0-or-later

//! The block layer: one struct per block type the mesh path needs, read
//! field by field in the order nif.xml gives for Bethesda 20.2.0.7 streams.
//! Every other block is kept as its raw bytes, found by the size table in
//! the header, so a file full of particle systems, controllers and Havok
//! collision still loads and still hands back its meshes.
//!
//! Node and shape families with trailing fields this crate does not need
//! (`BSMultiBoundNode`, `NiSwitchNode`, ...) are read as their base type and
//! the rest of the block is skipped by size, which is safe because every
//! subtype appends its fields after the base's.

use crate::controller::{
    read_float_data, read_path_interpolator, read_pos_data, read_transform_controller,
    read_transform_data, read_transform_interpolator, KeyGroup, PathInterpolator,
    TransformController, TransformData, TransformInterpolator,
};
use crate::error::{Error, Result};
use crate::header::{Header, BS_FALLOUT_4, BS_SKYRIM_SE};
use crate::stream::Stream;
use crate::types::{BlockRef, Bound, Transform, Vec3};
use crate::vertex::{read_vertex, Vertex, VertexDesc};

/// NiObjectNET: what every named object carries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectNet {
    pub name: String,
    pub extra_data: Vec<BlockRef>,
    pub controller: BlockRef,
}

/// NiAVObject: a placed object in the scene graph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AvObject {
    pub net: ObjectNet,
    pub flags: u32,
    pub transform: Transform,
    pub collision: BlockRef,
}

impl AvObject {
    pub fn name(&self) -> &str {
        &self.net.name
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub av: AvObject,
    pub children: Vec<BlockRef>,
    pub effects: Vec<BlockRef>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialData {
    pub names: Vec<String>,
    pub extra: Vec<i32>,
    pub active: i32,
    pub needs_update: bool,
}

/// NiTriShape and NiTriStrips: the pre-SSE geometry node whose data lives
/// in a separate NiTriShapeData or NiTriStripsData block.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriShape {
    pub av: AvObject,
    pub data: BlockRef,
    pub skin: BlockRef,
    pub material: MaterialData,
    pub shader: BlockRef,
    pub alpha: BlockRef,
    /// Authored `BSLODTriShape` size values for LOD0, LOD1, and LOD2.
    pub lod_sizes: Option<[u32; 3]>,
}

/// NiTriShapeData or NiTriStripsData, strips already unrolled to triangles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeometryData {
    pub vertices: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub tangents: Vec<Vec3>,
    pub bitangents: Vec<Vec3>,
    pub bound: Bound,
    pub colors: Vec<[f32; 4]>,
    /// One list per UV set, each `vertices.len()` long.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub bs_flags: u16,
    pub material_crc: u32,
    pub consistency: u16,
    pub additional_data: BlockRef,
    pub triangles: Vec<[u16; 3]>,
    /// True when the source was NiTriStripsData.
    pub from_strips: bool,
}

/// BSTriShape: the SSE and Fallout 4 geometry node with packed vertices
/// inline. A skinned SSE shape has `vertices` empty and its buffer in the
/// NiSkinPartition instead.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BsTriShape {
    pub av: AvObject,
    pub bound: Bound,
    pub skin: BlockRef,
    pub shader: BlockRef,
    pub alpha: BlockRef,
    pub desc: VertexDesc,
    pub num_triangles: u32,
    pub num_vertices: u16,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[u16; 3]>,
    /// Full precision dynamic positions from `BSDynamicTriShape`, including
    /// each source `Vector4`'s fourth component.
    pub dynamic_positions: Vec<[f32; 4]>,
}

/// BSLightingShaderType values for Skyrim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShaderType {
    #[default]
    Default,
    EnvironmentMap,
    Glow,
    Parallax,
    FaceTint,
    SkinTint,
    HairTint,
    ParallaxOcclusion,
    MultitextureLandscape,
    LodLandscape,
    Snow,
    MultiLayerParallax,
    TreeAnim,
    LodObjects,
    SparkleSnow,
    LodObjectsHd,
    EyeEnvmap,
    Cloud,
    LodLandscapeNoise,
    MultitextureLandscapeLodBlend,
    Dismemberment,
    Unknown(u32),
}

impl ShaderType {
    pub fn from_u32(v: u32) -> ShaderType {
        use ShaderType::*;
        match v {
            0 => Default,
            1 => EnvironmentMap,
            2 => Glow,
            3 => Parallax,
            4 => FaceTint,
            5 => SkinTint,
            6 => HairTint,
            7 => ParallaxOcclusion,
            8 => MultitextureLandscape,
            9 => LodLandscape,
            10 => Snow,
            11 => MultiLayerParallax,
            12 => TreeAnim,
            13 => LodObjects,
            14 => SparkleSnow,
            15 => LodObjectsHd,
            16 => EyeEnvmap,
            17 => Cloud,
            18 => LodLandscapeNoise,
            19 => MultitextureLandscapeLodBlend,
            20 => Dismemberment,
            other => Unknown(other),
        }
    }
}

/// The per-shader-type tail of BSLightingShaderProperty.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ShaderExtra {
    #[default]
    None,
    EnvironmentMapScale(f32),
    SkinTint([f32; 3]),
    HairTint([f32; 3]),
    ParallaxOcclusion {
        max_passes: f32,
        scale: f32,
    },
    MultiLayerParallax {
        inner_layer_thickness: f32,
        refraction_scale: f32,
        inner_layer_texture_scale: [f32; 2],
        envmap_strength: f32,
    },
    SparkleSnow([f32; 4]),
    EyeEnvmap {
        cubemap_scale: f32,
        left_eye_center: Vec3,
        right_eye_center: Vec3,
    },
}

/// Selected SkyrimShaderPropertyFlags1 bits.
pub mod flags1 {
    pub const SPECULAR: u32 = 1 << 0;
    pub const SKINNED: u32 = 1 << 1;
    pub const VERTEX_ALPHA: u32 = 1 << 3;
    pub const GREYSCALE_TO_PALETTE_COLOR: u32 = 1 << 4;
    pub const GREYSCALE_TO_PALETTE_ALPHA: u32 = 1 << 5;
    pub const USE_FALLOFF: u32 = 1 << 6;
    pub const ENVIRONMENT_MAPPING: u32 = 1 << 7;
    pub const RECEIVE_SHADOWS: u32 = 1 << 8;
    pub const CAST_SHADOWS: u32 = 1 << 9;
    pub const FACEGEN_DETAIL_MAP: u32 = 1 << 10;
    pub const PARALLAX: u32 = 1 << 11;
    pub const MODEL_SPACE_NORMALS: u32 = 1 << 12;
    pub const LANDSCAPE: u32 = 1 << 14;
    pub const REFRACTION: u32 = 1 << 15;
    pub const EYE_ENVIRONMENT_MAPPING: u32 = 1 << 17;
    pub const HAIR_SOFT_LIGHTING: u32 = 1 << 18;
    pub const FACEGEN_RGB_TINT: u32 = 1 << 21;
    pub const OWN_EMIT: u32 = 1 << 22;
    pub const PROJECTED_UV: u32 = 1 << 23;
    pub const DECAL: u32 = 1 << 26;
    pub const DYNAMIC_DECAL: u32 = 1 << 27;
    pub const PARALLAX_OCCLUSION: u32 = 1 << 28;
    pub const EXTERNAL_EMITTANCE: u32 = 1 << 29;
    pub const SOFT_EFFECT: u32 = 1 << 30;
    pub const ZBUFFER_TEST: u32 = 1 << 31;
}

/// Selected SkyrimShaderPropertyFlags2 bits.
pub mod flags2 {
    pub const ZBUFFER_WRITE: u32 = 1 << 0;
    pub const LOD_LANDSCAPE: u32 = 1 << 1;
    pub const LOD_OBJECTS: u32 = 1 << 2;
    pub const NO_FADE: u32 = 1 << 3;
    pub const DOUBLE_SIDED: u32 = 1 << 4;
    pub const VERTEX_COLORS: u32 = 1 << 5;
    pub const GLOW_MAP: u32 = 1 << 6;
    pub const ASSUME_SHADOWMASK: u32 = 1 << 7;
    pub const PACKED_TANGENT: u32 = 1 << 8;
    pub const MULTI_INDEX_SNOW: u32 = 1 << 9;
    pub const VERTEX_LIGHTING: u32 = 1 << 10;
    pub const BILLBOARD: u32 = 1 << 13;
    pub const ENVMAP_LIGHT_FADE: u32 = 1 << 15;
    pub const WEAPON_BLOOD: u32 = 1 << 17;
    pub const HIDE_ON_LOCAL_MAP: u32 = 1 << 18;
    pub const PREMULT_ALPHA: u32 = 1 << 19;
    pub const ANISOTROPIC_LIGHTING: u32 = 1 << 21;
    pub const MULTI_LAYER_PARALLAX: u32 = 1 << 24;
    pub const SOFT_LIGHTING: u32 = 1 << 25;
    pub const RIM_LIGHTING: u32 = 1 << 26;
    pub const BACK_LIGHTING: u32 = 1 << 27;
    pub const TREE_ANIM: u32 = 1 << 29;
    pub const EFFECT_LIGHTING: u32 = 1 << 30;
    pub const HD_LOD_OBJECTS: u32 = 1 << 31;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LightingShader {
    pub net: ObjectNet,
    pub shader_type: ShaderType,
    pub flags1: u32,
    pub flags2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub texture_set: BlockRef,
    pub emissive_color: [f32; 3],
    pub emissive_multiple: f32,
    /// Fallout 4 only: the BGSM path.
    pub root_material: String,
    pub clamp_mode: u32,
    pub alpha: f32,
    pub refraction_strength: f32,
    /// Specular power on Skyrim, smoothness on Fallout 4.
    pub glossiness: f32,
    pub specular_color: [f32; 3],
    pub specular_strength: f32,
    pub lighting_effect: [f32; 2],
    pub extra: ShaderExtra,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectShader {
    pub net: ObjectNet,
    pub flags1: u32,
    pub flags2: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub source_texture: String,
    pub clamp_mode: u8,
    pub lighting_influence: u8,
    pub env_map_min_lod: u8,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    pub base_color: [f32; 4],
    pub base_color_scale: f32,
    pub soft_falloff_depth: f32,
    pub greyscale_texture: String,
    pub env_map_texture: String,
    pub normal_texture: String,
    pub env_mask_texture: String,
    pub env_map_scale: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextureSet {
    /// Skyrim slot order: diffuse, normal, glow or subsurface, height,
    /// environment cube, environment mask, inner layer or face tint,
    /// specular or backlight. Empty strings are unused slots.
    pub textures: Vec<String>,
}

impl TextureSet {
    pub fn slot(&self, i: usize) -> Option<&str> {
        self.textures
            .get(i)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }
}

/// Blend factors from the NiAlphaProperty flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    One,
    Zero,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
    SrcAlphaSaturate,
}

impl BlendFactor {
    fn from_bits(v: u16) -> BlendFactor {
        use BlendFactor::*;
        match v & 0xF {
            0 => One,
            1 => Zero,
            2 => SrcColor,
            3 => OneMinusSrcColor,
            4 => DstColor,
            5 => OneMinusDstColor,
            6 => SrcAlpha,
            7 => OneMinusSrcAlpha,
            8 => DstAlpha,
            9 => OneMinusDstAlpha,
            _ => SrcAlphaSaturate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestFunc {
    Always,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Never,
}

impl TestFunc {
    fn from_bits(v: u16) -> TestFunc {
        use TestFunc::*;
        match v & 0x7 {
            0 => Always,
            1 => Less,
            2 => Equal,
            3 => LessEqual,
            4 => Greater,
            5 => NotEqual,
            6 => GreaterEqual,
            _ => Never,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlphaProperty {
    pub net: ObjectNet,
    pub flags: u16,
    pub threshold: u8,
}

impl AlphaProperty {
    pub fn blend_enabled(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn src_blend(&self) -> BlendFactor {
        BlendFactor::from_bits(self.flags >> 1)
    }

    pub fn dst_blend(&self) -> BlendFactor {
        BlendFactor::from_bits(self.flags >> 5)
    }

    pub fn test_enabled(&self) -> bool {
        self.flags & (1 << 9) != 0
    }

    pub fn test_func(&self) -> TestFunc {
        TestFunc::from_bits(self.flags >> 10)
    }

    pub fn no_sorter(&self) -> bool {
        self.flags & (1 << 13) != 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BodyPart {
    pub flags: u16,
    pub part: u16,
}

/// NiSkinInstance and BSDismemberSkinInstance.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinInstance {
    pub data: BlockRef,
    pub partition: BlockRef,
    pub skeleton_root: BlockRef,
    pub bones: Vec<BlockRef>,
    /// Present on BSDismemberSkinInstance, one per partition.
    pub body_parts: Vec<BodyPart>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinBone {
    /// Offset of the skin from this bone in bind pose.
    pub transform: Transform,
    pub bound: Bound,
    /// (vertex index, weight) pairs, empty when the file only carries
    /// weights in the partition.
    pub weights: Vec<(u16, f32)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinData {
    pub transform: Transform,
    pub bones: Vec<SkinBone>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Partition {
    pub bones_per_vertex: u16,
    /// Indices into the skin instance's bone list.
    pub bones: Vec<u16>,
    /// Partition-local vertex to shape vertex.
    pub vertex_map: Vec<u16>,
    /// `vertices * bones_per_vertex` weights, partition-local order.
    pub weights: Vec<f32>,
    /// Partition-local triangles, strips already unrolled.
    pub triangles: Vec<[u16; 3]>,
    /// `vertices * bones_per_vertex` indices into `bones`.
    pub bone_indices: Vec<u8>,
    pub lod_level: u8,
    pub global_vb: bool,
    pub desc: VertexDesc,
    /// SSE: the same triangles in shape vertex indices.
    pub triangles_copy: Vec<[u16; 3]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinPartition {
    pub desc: VertexDesc,
    /// SSE global vertex buffer for the skinned shape.
    pub vertices: Vec<Vertex>,
    pub partitions: Vec<Partition>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Node(Node),
    TriShape(TriShape),
    GeometryData(GeometryData),
    BsTriShape(BsTriShape),
    LightingShader(LightingShader),
    EffectShader(EffectShader),
    TextureSet(TextureSet),
    AlphaProperty(AlphaProperty),
    SkinInstance(SkinInstance),
    SkinData(SkinData),
    SkinPartition(SkinPartition),
    /// `NiTransformController` and `NiKeyframeController`: drives a target
    /// node's transform through an interpolator.
    TransformController(TransformController),
    /// `NiTransformInterpolator`: default transform plus an optional key
    /// track.
    TransformInterpolator(TransformInterpolator),
    /// `NiPathInterpolator`: follows a position curve, parsed but not yet
    /// evaluated, see `docs/nif.md`.
    PathInterpolator(PathInterpolator),
    /// `NiTransformData` and `NiKeyframeData`: the rotation, translation and
    /// scale key tracks a `TransformInterpolator` plays back.
    TransformData(TransformData),
    /// `NiPosData`: a position key track, used by `NiPathInterpolator`.
    PosData(KeyGroup<Vec3>),
    /// `NiFloatData`: a float key track, used by `NiPathInterpolator` and by
    /// XYZ Euler rotation channels inlined in `TransformData` instead.
    FloatData(KeyGroup<f32>),
    /// bhk* Havok collision, kept opaque for the physics layer.
    Collision {
        type_name: String,
        data: Vec<u8>,
    },
    /// Anything else, kept so a later pass can decode it.
    Unknown {
        type_name: String,
        data: Vec<u8>,
    },
}

impl Block {
    pub fn type_name(&self) -> &str {
        match self {
            Block::Node(_) => "NiNode",
            Block::TriShape(_) => "NiTriShape",
            Block::GeometryData(_) => "NiTriShapeData",
            Block::BsTriShape(_) => "BSTriShape",
            Block::LightingShader(_) => "BSLightingShaderProperty",
            Block::EffectShader(_) => "BSEffectShaderProperty",
            Block::TextureSet(_) => "BSShaderTextureSet",
            Block::AlphaProperty(_) => "NiAlphaProperty",
            Block::SkinInstance(_) => "NiSkinInstance",
            Block::SkinData(_) => "NiSkinData",
            Block::SkinPartition(_) => "NiSkinPartition",
            Block::TransformController(_) => "NiTransformController",
            Block::TransformInterpolator(_) => "NiTransformInterpolator",
            Block::PathInterpolator(_) => "NiPathInterpolator",
            Block::TransformData(_) => "NiTransformData",
            Block::PosData(_) => "NiPosData",
            Block::FloatData(_) => "NiFloatData",
            Block::Collision { type_name, .. } | Block::Unknown { type_name, .. } => type_name,
        }
    }

    pub fn as_node(&self) -> Option<&Node> {
        match self {
            Block::Node(n) => Some(n),
            _ => None,
        }
    }
}

/// Block types read as NiNode with any trailing fields skipped.
const NODE_FAMILY: &[&str] = &[
    "NiNode",
    "BSFadeNode",
    "BSLeafAnimNode",
    "BSTreeNode",
    "BSOrderedNode",
    "BSMultiBoundNode",
    "BSValueNode",
    "BSBlastNode",
    "BSDamageStage",
    "BSDebrisNode",
    "BSRangeNode",
    "BSMasterParticleSystem",
    "NiBillboardNode",
    "NiSwitchNode",
    "NiLODNode",
    "NiBSAnimationNode",
    "NiBSParticleNode",
    "NiSortAdjustNode",
];

/// Block types read as NiTriShape.
const TRISHAPE_FAMILY: &[&str] = &[
    "NiTriShape",
    "NiTriStrips",
    "BSLODTriShape",
    "BSSegmentedTriShape",
];

/// Block types read as BSTriShape.
const BSTRISHAPE_FAMILY: &[&str] = &[
    "BSTriShape",
    "BSDynamicTriShape",
    "BSSubIndexTriShape",
    "BSMeshLODTriShape",
];

enum Kind {
    Node,
    TriShape,
    TriShapeData,
    TriStripsData,
    BsTriShape,
    LightingShader,
    EffectShader,
    TextureSet,
    AlphaProperty,
    SkinInstance,
    DismemberSkinInstance,
    SkinData,
    SkinPartition,
    TransformController,
    TransformInterpolator,
    PathInterpolator,
    TransformData,
    PosData,
    FloatData,
    Collision,
    Unknown,
}

fn kind_of(type_name: &str) -> Kind {
    if NODE_FAMILY.contains(&type_name) {
        return Kind::Node;
    }
    if TRISHAPE_FAMILY.contains(&type_name) {
        return Kind::TriShape;
    }
    if BSTRISHAPE_FAMILY.contains(&type_name) {
        return Kind::BsTriShape;
    }
    match type_name {
        "NiTriShapeData" => Kind::TriShapeData,
        "NiTriStripsData" => Kind::TriStripsData,
        "BSLightingShaderProperty" => Kind::LightingShader,
        "BSEffectShaderProperty" => Kind::EffectShader,
        "BSShaderTextureSet" => Kind::TextureSet,
        "NiAlphaProperty" => Kind::AlphaProperty,
        "NiSkinInstance" => Kind::SkinInstance,
        "BSDismemberSkinInstance" => Kind::DismemberSkinInstance,
        "NiSkinData" => Kind::SkinData,
        "NiSkinPartition" => Kind::SkinPartition,
        // `NiTransformController` is the current name. `NiKeyframeController`
        // is the pre-10.2 name it was renamed from, and both have the same
        // fields at our fixed file version (nif.xml lines 3600-3665).
        "NiTransformController" | "NiKeyframeController" => Kind::TransformController,
        "NiTransformInterpolator" => Kind::TransformInterpolator,
        "NiPathInterpolator" => Kind::PathInterpolator,
        // Likewise `NiTransformData` renamed from `NiKeyframeData`, same
        // layout at our version (nif.xml lines 4327-4337).
        "NiTransformData" | "NiKeyframeData" => Kind::TransformData,
        "NiPosData" => Kind::PosData,
        "NiFloatData" => Kind::FloatData,
        t if t.starts_with("bhk") => Kind::Collision,
        _ => Kind::Unknown,
    }
}

struct Ctx<'h> {
    h: &'h Header,
    block: usize,
}

impl Ctx<'_> {
    fn invalid(&self, reason: impl Into<String>) -> Error {
        Error::InvalidData {
            block: self.block,
            reason: reason.into(),
        }
    }

    fn triangles(&self, s: &mut Stream<'_>, n: usize) -> Result<Vec<[u16; 3]>> {
        s.count(n as u32, 6)?;
        (0..n).map(|_| Ok([s.u16()?, s.u16()?, s.u16()?])).collect()
    }
}

fn read_object_net(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<ObjectNet> {
    Ok(ObjectNet {
        name: s.string_index(&c.h.strings)?,
        extra_data: s.block_refs()?,
        controller: s.block_ref()?,
    })
}

fn read_av_object(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<AvObject> {
    let net = read_object_net(s, c)?;
    let flags = if c.h.bs_version > 26 {
        s.u32()?
    } else {
        s.u16()? as u32
    };
    let translation = s.vec3()?;
    let rotation = s.mat33()?;
    let scale = s.f32()?;
    // NiAVObject carried a property list up to Fallout 3 (BS version 34).
    if c.h.bs_version <= 34 {
        s.block_refs()?;
    }
    let collision = s.block_ref()?;
    Ok(AvObject {
        net,
        flags,
        transform: Transform {
            rotation,
            translation,
            scale,
        },
        collision,
    })
}

fn read_node(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<Node> {
    let av = read_av_object(s, c)?;
    let children = s.block_refs()?;
    let effects = if c.h.bs_version < BS_FALLOUT_4 {
        s.block_refs()?
    } else {
        Vec::new()
    };
    Ok(Node {
        av,
        children,
        effects,
    })
}

fn read_material_data(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<MaterialData> {
    let n = s.counted(8)?;
    // The name index array comes first, then all the extra data ints.
    let mut names = Vec::with_capacity(n);
    for _ in 0..n {
        names.push(s.string_index(&c.h.strings)?);
    }
    let mut extra = Vec::with_capacity(n);
    for _ in 0..n {
        extra.push(s.i32()?);
    }
    Ok(MaterialData {
        names,
        extra,
        active: s.i32()?,
        needs_update: s.bool()?,
    })
}

fn read_tri_shape(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<TriShape> {
    let av = read_av_object(s, c)?;
    let data = s.block_ref()?;
    let skin = s.block_ref()?;
    let material = read_material_data(s, c)?;
    let (shader, alpha) = if c.h.bs_version > 34 {
        (s.block_ref()?, s.block_ref()?)
    } else {
        (BlockRef::NONE, BlockRef::NONE)
    };
    Ok(TriShape {
        av,
        data,
        skin,
        material,
        shader,
        alpha,
        lod_sizes: None,
    })
}

fn read_bs_lod_tri_shape(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<TriShape> {
    let mut shape = read_tri_shape(s, c)?;
    shape.lod_sizes = Some([s.u32()?, s.u32()?, s.u32()?]);
    Ok(shape)
}

fn read_vec3s(s: &mut Stream<'_>, n: usize) -> Result<Vec<Vec3>> {
    s.count(n as u32, 12)?;
    (0..n).map(|_| s.vec3()).collect()
}

fn read_geometry_data(s: &mut Stream<'_>, c: &Ctx<'_>, strips: bool) -> Result<GeometryData> {
    let mut g = GeometryData {
        from_strips: strips,
        ..Default::default()
    };
    let _group_id = s.i32()?;
    let num_vertices = s.u16()? as usize;
    let _keep_flags = s.u8()?;
    let _compress_flags = s.u8()?;
    if s.bool()? {
        g.vertices = read_vec3s(s, num_vertices)?;
    }
    g.bs_flags = s.u16()?;
    if c.h.bs_version > 34 {
        g.material_crc = s.u32()?;
    }
    let has_normals = s.bool()?;
    if has_normals {
        g.normals = read_vec3s(s, num_vertices)?;
        if g.bs_flags & 4096 != 0 {
            g.tangents = read_vec3s(s, num_vertices)?;
            g.bitangents = read_vec3s(s, num_vertices)?;
        }
    }
    g.bound = s.bound()?;
    if s.bool()? {
        s.count(num_vertices as u32, 16)?;
        for _ in 0..num_vertices {
            g.colors.push(s.color4()?);
        }
    }
    let uv_sets = (g.bs_flags & 1) as usize;
    for _ in 0..uv_sets {
        s.count(num_vertices as u32, 8)?;
        let mut set = Vec::with_capacity(num_vertices);
        for _ in 0..num_vertices {
            set.push(s.vec2()?);
        }
        g.uv_sets.push(set);
    }
    g.consistency = s.u16()?;
    g.additional_data = s.block_ref()?;
    let num_triangles = s.u16()? as usize;
    if strips {
        let num_strips = s.u16()? as usize;
        s.count(num_strips as u32, 2)?;
        let mut lengths = Vec::with_capacity(num_strips);
        for _ in 0..num_strips {
            lengths.push(s.u16()? as usize);
        }
        if s.bool()? {
            for &len in &lengths {
                s.count(len as u32, 2)?;
                let mut strip = Vec::with_capacity(len);
                for _ in 0..len {
                    strip.push(s.u16()?);
                }
                unroll_strip(&strip, &mut g.triangles);
            }
        }
    } else {
        let _num_triangle_points = s.u32()?;
        if s.bool()? {
            g.triangles = c.triangles(s, num_triangles)?;
        }
        let match_groups = s.u16()? as usize;
        for _ in 0..match_groups {
            let n = s.counted16(2)?;
            s.take(n * 2)?;
        }
    }
    for (i, tri) in g.triangles.iter().enumerate() {
        if tri.iter().any(|&v| v as usize >= num_vertices) {
            return Err(c.invalid(format!(
                "triangle {i} indexes past the {num_vertices} vertices"
            )));
        }
    }
    Ok(g)
}

/// Triangle strip to list, skipping degenerate triangles and alternating
/// winding so every output triangle faces the way the first one does.
pub fn unroll_strip(strip: &[u16], out: &mut Vec<[u16; 3]>) {
    for i in 0..strip.len().saturating_sub(2) {
        let (a, b, c) = (strip[i], strip[i + 1], strip[i + 2]);
        if a == b || b == c || a == c {
            continue;
        }
        if i % 2 == 0 {
            out.push([a, b, c]);
        } else {
            out.push([b, a, c]);
        }
    }
}

fn read_vertices(
    s: &mut Stream<'_>,
    c: &Ctx<'_>,
    desc: VertexDesc,
    n: usize,
) -> Result<Vec<Vertex>> {
    let attrs = desc.attributes();
    let size = desc.size_bytes();
    let expected = crate::vertex::vertex_size(attrs, c.h.bs_version);
    if size != expected {
        return Err(c.invalid(format!(
            "vertex descriptor says {size} bytes per vertex but its attributes {attrs:#x} need {expected}"
        )));
    }
    s.count(n as u32, size.max(1))?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(read_vertex(s, attrs, c.h)?);
    }
    Ok(out)
}

fn read_bs_tri_shape(s: &mut Stream<'_>, c: &Ctx<'_>, dynamic: bool) -> Result<BsTriShape> {
    let av = read_av_object(s, c)?;
    let bound = s.bound()?;
    if c.h.bs_version >= 155 {
        s.take(24)?;
    }
    let skin = s.block_ref()?;
    let shader = s.block_ref()?;
    let alpha = s.block_ref()?;
    let desc = VertexDesc(s.u64()?);
    let num_triangles = if c.h.bs_version >= BS_FALLOUT_4 {
        s.u32()?
    } else {
        s.u16()? as u32
    };
    let num_vertices = s.u16()?;
    let data_size = s.u32()?;
    let mut t = BsTriShape {
        av,
        bound,
        skin,
        shader,
        alpha,
        desc,
        num_triangles,
        num_vertices,
        vertices: Vec::new(),
        triangles: Vec::new(),
        dynamic_positions: Vec::new(),
    };
    if data_size > 0 {
        t.vertices = read_vertices(s, c, desc, num_vertices as usize)?;
        t.triangles = c.triangles(s, num_triangles as usize)?;
        for (i, tri) in t.triangles.iter().enumerate() {
            if tri.iter().any(|&v| v >= num_vertices) {
                return Err(c.invalid(format!(
                    "triangle {i} indexes past the {num_vertices} vertices"
                )));
            }
        }
    }
    if c.h.bs_version == BS_SKYRIM_SE {
        let particle_size = s.u32()?;
        if particle_size > 0 {
            // Particle copies of the geometry as halves, not needed here.
            let nv = num_vertices as usize;
            s.count(nv as u32, 12)?;
            s.take(nv * 12)?;
            s.count(num_triangles, 6)?;
            s.take(num_triangles as usize * 6)?;
        }
    }
    if dynamic {
        let dynamic_data_size = s.u32()?;
        let expected_size = u32::from(num_vertices) * 16;
        if dynamic_data_size != expected_size {
            return Err(c.invalid(format!(
                "dynamic data size {dynamic_data_size} does not match {num_vertices} vertices ({expected_size} bytes)"
            )));
        }
        s.count(u32::from(num_vertices), 16)?;
        for _ in 0..num_vertices {
            t.dynamic_positions
                .push([s.f32()?, s.f32()?, s.f32()?, s.f32()?]);
        }
    }
    Ok(t)
}

fn read_lighting_shader(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<LightingShader> {
    // The shader type is written before the object's name.
    let shader_type = if c.h.bs_version < 155 {
        ShaderType::from_u32(s.u32()?)
    } else {
        ShaderType::Default
    };
    let net = read_object_net(s, c)?;
    let mut p = LightingShader {
        net,
        shader_type,
        ..Default::default()
    };
    if c.h.bs_version > 131 {
        return Err(c.invalid("Fallout 76 shader flag hashes are not supported"));
    }
    p.flags1 = s.u32()?;
    p.flags2 = s.u32()?;
    p.uv_offset = s.vec2()?;
    p.uv_scale = s.vec2()?;
    p.texture_set = s.block_ref()?;
    p.emissive_color = s.color3()?;
    p.emissive_multiple = s.f32()?;
    if c.h.bs_version >= BS_FALLOUT_4 {
        p.root_material = s.string_index(&c.h.strings)?;
    }
    p.clamp_mode = s.u32()?;
    p.alpha = s.f32()?;
    p.refraction_strength = s.f32()?;
    p.glossiness = s.f32()?;
    p.specular_color = s.color3()?;
    p.specular_strength = s.f32()?;
    if c.h.bs_version >= BS_FALLOUT_4 {
        // Subsurface, rim, wetness and the per-type tail differ per Fallout
        // 4 revision. The block is skipped to its recorded end by the caller.
        return Ok(p);
    }
    p.lighting_effect = [s.f32()?, s.f32()?];
    p.extra = match shader_type {
        ShaderType::EnvironmentMap => ShaderExtra::EnvironmentMapScale(s.f32()?),
        ShaderType::SkinTint => ShaderExtra::SkinTint(s.color3()?),
        ShaderType::HairTint => ShaderExtra::HairTint(s.color3()?),
        ShaderType::ParallaxOcclusion => ShaderExtra::ParallaxOcclusion {
            max_passes: s.f32()?,
            scale: s.f32()?,
        },
        ShaderType::MultiLayerParallax => ShaderExtra::MultiLayerParallax {
            inner_layer_thickness: s.f32()?,
            refraction_scale: s.f32()?,
            inner_layer_texture_scale: s.vec2()?,
            envmap_strength: s.f32()?,
        },
        ShaderType::SparkleSnow => ShaderExtra::SparkleSnow(s.color4()?),
        ShaderType::EyeEnvmap => ShaderExtra::EyeEnvmap {
            cubemap_scale: s.f32()?,
            left_eye_center: s.vec3()?,
            right_eye_center: s.vec3()?,
        },
        _ => ShaderExtra::None,
    };
    Ok(p)
}

fn read_effect_shader(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<EffectShader> {
    let net = read_object_net(s, c)?;
    if c.h.bs_version > 131 {
        return Err(c.invalid("Fallout 76 shader flag hashes are not supported"));
    }
    let mut p = EffectShader {
        net,
        ..Default::default()
    };
    p.flags1 = s.u32()?;
    p.flags2 = s.u32()?;
    p.uv_offset = s.vec2()?;
    p.uv_scale = s.vec2()?;
    p.source_texture = s.sized_string()?;
    p.clamp_mode = s.u8()?;
    p.lighting_influence = s.u8()?;
    p.env_map_min_lod = s.u8()?;
    let _unused = s.u8()?;
    p.falloff_start_angle = s.f32()?;
    p.falloff_stop_angle = s.f32()?;
    p.falloff_start_opacity = s.f32()?;
    p.falloff_stop_opacity = s.f32()?;
    p.base_color = s.color4()?;
    p.base_color_scale = s.f32()?;
    p.soft_falloff_depth = s.f32()?;
    p.greyscale_texture = s.sized_string()?;
    if c.h.bs_version >= BS_FALLOUT_4 {
        p.env_map_texture = s.sized_string()?;
        p.normal_texture = s.sized_string()?;
        p.env_mask_texture = s.sized_string()?;
        p.env_map_scale = s.f32()?;
    }
    Ok(p)
}

fn read_texture_set(s: &mut Stream<'_>, _c: &Ctx<'_>) -> Result<TextureSet> {
    let n = s.counted(4)?;
    let mut textures = Vec::with_capacity(n);
    for _ in 0..n {
        textures.push(s.sized_string()?);
    }
    Ok(TextureSet { textures })
}

fn read_alpha_property(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<AlphaProperty> {
    let net = read_object_net(s, c)?;
    Ok(AlphaProperty {
        net,
        flags: s.u16()?,
        threshold: s.u8()?,
    })
}

fn read_skin_instance(s: &mut Stream<'_>, _c: &Ctx<'_>, dismember: bool) -> Result<SkinInstance> {
    let data = s.block_ref()?;
    let partition = s.block_ref()?;
    let skeleton_root = s.block_ref()?;
    let bone_count = s.u32()?;
    // Skyrim skeleton files use u32::MAX for an empty bone-reference array.
    let bones = if bone_count == u32::MAX {
        Vec::new()
    } else {
        let n = s.count(bone_count, 4)?;
        (0..n).map(|_| s.block_ref()).collect::<Result<Vec<_>>>()?
    };
    let mut body_parts = Vec::new();
    if dismember {
        let n = s.counted(4)?;
        for _ in 0..n {
            body_parts.push(BodyPart {
                flags: s.u16()?,
                part: s.u16()?,
            });
        }
    }
    Ok(SkinInstance {
        data,
        partition,
        skeleton_root,
        bones,
        body_parts,
    })
}

fn read_skin_data(s: &mut Stream<'_>, _c: &Ctx<'_>) -> Result<SkinData> {
    let transform = s.transform()?;
    let n = s.counted(52 + 16 + 2)?;
    let has_weights = s.bool()?;
    let mut bones = Vec::with_capacity(n);
    for _ in 0..n {
        let transform = s.transform()?;
        let bound = s.bound()?;
        let nv = s.u16()? as usize;
        let mut weights = Vec::new();
        if has_weights {
            s.count(nv as u32, 6)?;
            for _ in 0..nv {
                weights.push((s.u16()?, s.f32()?));
            }
        }
        bones.push(SkinBone {
            transform,
            bound,
            weights,
        });
    }
    Ok(SkinData { transform, bones })
}

fn read_partition(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<Partition> {
    let nv = s.u16()? as usize;
    let nt = s.u16()? as usize;
    let nb = s.u16()? as usize;
    let ns = s.u16()? as usize;
    let bpv = s.u16()?;
    let mut p = Partition {
        bones_per_vertex: bpv,
        ..Default::default()
    };
    s.count(nb as u32, 2)?;
    for _ in 0..nb {
        p.bones.push(s.u16()?);
    }
    if s.bool()? {
        s.count(nv as u32, 2)?;
        for _ in 0..nv {
            p.vertex_map.push(s.u16()?);
        }
    }
    if s.bool()? {
        let n = nv * bpv as usize;
        s.count(n as u32, 4)?;
        for _ in 0..n {
            p.weights.push(s.f32()?);
        }
    }
    s.count(ns as u32, 2)?;
    let mut strip_lengths = Vec::with_capacity(ns);
    for _ in 0..ns {
        strip_lengths.push(s.u16()? as usize);
    }
    if s.bool()? {
        if ns > 0 {
            for &len in &strip_lengths {
                s.count(len as u32, 2)?;
                let mut strip = Vec::with_capacity(len);
                for _ in 0..len {
                    strip.push(s.u16()?);
                }
                unroll_strip(&strip, &mut p.triangles);
            }
        } else {
            p.triangles = c.triangles(s, nt)?;
        }
    }
    if s.bool()? {
        let n = nv * bpv as usize;
        p.bone_indices = s.take(n)?.to_vec();
    }
    if c.h.bs_version > 34 {
        p.lod_level = s.u8()?;
        p.global_vb = s.bool()?;
        if c.h.bs_version == BS_SKYRIM_SE {
            p.desc = VertexDesc(s.u64()?);
            p.triangles_copy = c.triangles(s, nt)?;
        }
    }
    Ok(p)
}

fn read_skin_partition(s: &mut Stream<'_>, c: &Ctx<'_>) -> Result<SkinPartition> {
    let n = s.counted(10)?;
    let mut sp = SkinPartition::default();
    if c.h.bs_version == BS_SKYRIM_SE {
        let data_size = s.u32()?;
        let vertex_size = s.u32()?;
        sp.desc = VertexDesc(s.u64()?);
        if data_size > 0 {
            if vertex_size == 0 || vertex_size as usize != sp.desc.size_bytes() {
                return Err(c.invalid(format!(
                    "skin partition vertex size {vertex_size} does not match its descriptor"
                )));
            }
            let count = (data_size / vertex_size) as usize;
            sp.vertices = read_vertices(s, c, sp.desc, count)?;
        }
    }
    for _ in 0..n {
        sp.partitions.push(read_partition(s, c)?);
    }
    Ok(sp)
}

/// Read every block the header describes. Returns the blocks in file order.
pub fn read_blocks(s: &mut Stream<'_>, h: &Header) -> Result<Vec<Block>> {
    let mut blocks = Vec::with_capacity(h.block_count());
    for i in 0..h.block_count() {
        let type_name = h.block_type(i).to_string();
        let size = h.block_sizes[i] as usize;
        let start = s.pos();
        if size > s.remaining() {
            return Err(Error::Truncated {
                offset: start,
                needed: size - s.remaining(),
            });
        }
        let end = start + size;
        let c = Ctx { h, block: i };
        // Whether the reader consumed the whole block by itself. Family
        // members with trailing fields are read as a prefix and skipped.
        let exact = matches!(
            type_name.as_str(),
            "NiNode"
                | "BSFadeNode"
                | "NiTriShape"
                | "NiTriStrips"
                | "BSLODTriShape"
                | "NiTriShapeData"
                | "NiTriStripsData"
                | "BSTriShape"
                | "BSDynamicTriShape"
                | "BSEffectShaderProperty"
                | "BSShaderTextureSet"
                | "NiAlphaProperty"
                | "NiSkinInstance"
                | "BSDismemberSkinInstance"
                | "NiSkinData"
                | "NiSkinPartition"
                | "NiTransformController"
                | "NiKeyframeController"
                | "NiTransformInterpolator"
                | "NiPathInterpolator"
                | "NiTransformData"
                | "NiKeyframeData"
                | "NiPosData"
                | "NiFloatData"
        ) || (type_name == "BSLightingShaderProperty" && h.bs_version < BS_FALLOUT_4);
        let block = match kind_of(&type_name) {
            Kind::Node => Block::Node(read_node(s, &c)?),
            Kind::TriShape if type_name == "BSLODTriShape" => {
                Block::TriShape(read_bs_lod_tri_shape(s, &c)?)
            }
            Kind::TriShape => Block::TriShape(read_tri_shape(s, &c)?),
            Kind::TriShapeData => Block::GeometryData(read_geometry_data(s, &c, false)?),
            Kind::TriStripsData => Block::GeometryData(read_geometry_data(s, &c, true)?),
            Kind::BsTriShape => {
                Block::BsTriShape(read_bs_tri_shape(s, &c, type_name == "BSDynamicTriShape")?)
            }
            Kind::LightingShader => Block::LightingShader(read_lighting_shader(s, &c)?),
            Kind::EffectShader => Block::EffectShader(read_effect_shader(s, &c)?),
            Kind::TextureSet => Block::TextureSet(read_texture_set(s, &c)?),
            Kind::AlphaProperty => Block::AlphaProperty(read_alpha_property(s, &c)?),
            Kind::SkinInstance => Block::SkinInstance(read_skin_instance(s, &c, false)?),
            Kind::DismemberSkinInstance => Block::SkinInstance(read_skin_instance(s, &c, true)?),
            Kind::SkinData => Block::SkinData(read_skin_data(s, &c)?),
            Kind::SkinPartition => Block::SkinPartition(read_skin_partition(s, &c)?),
            Kind::TransformController => Block::TransformController(read_transform_controller(s)?),
            Kind::TransformInterpolator => {
                Block::TransformInterpolator(read_transform_interpolator(s)?)
            }
            Kind::PathInterpolator => Block::PathInterpolator(read_path_interpolator(s)?),
            Kind::TransformData => Block::TransformData(read_transform_data(s, i)?),
            Kind::PosData => Block::PosData(read_pos_data(s, i)?),
            Kind::FloatData => Block::FloatData(read_float_data(s, i)?),
            Kind::Collision | Kind::Unknown => {
                if size == 0 {
                    return Err(c.invalid(format!(
                        "block type {type_name} is not parsed and the header gives it no size"
                    )));
                }
                let data = s.take(size)?.to_vec();
                let name = type_name.clone();
                if matches!(kind_of(&type_name), Kind::Collision) {
                    Block::Collision {
                        type_name: name,
                        data,
                    }
                } else {
                    Block::Unknown {
                        type_name: name,
                        data,
                    }
                }
            }
        };
        if size > 0 {
            let actual = s.pos();
            if actual > end || (exact && actual != end) {
                return Err(Error::BlockSize {
                    index: i,
                    type_name,
                    expected_end: end,
                    actual_end: actual,
                });
            }
            s.seek(end)?;
        }
        blocks.push(block);
    }
    Ok(blocks)
}

/// The footer: root block references.
pub fn read_footer(s: &mut Stream<'_>) -> Result<Vec<BlockRef>> {
    s.block_refs()
}
