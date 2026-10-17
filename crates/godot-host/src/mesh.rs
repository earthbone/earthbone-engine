// SPDX-License-Identifier: GPL-3.0-or-later

//! From a decoded NIF shape to the arrays Godot's `ArrayMesh` takes.
//!
//! The `nif` crate is being written in parallel, so this module codes
//! against `MeshSource`, a trait shaped like its neutral output struct. When
//! that crate lands, an `impl MeshSource for soe_nif::Shape` is the whole
//! integration. Everything here is plain Rust: axis conversion, winding,
//! normal generation, validation, bounds. The Godot side turns a
//! `SurfaceArrays` into packed arrays and nothing else.

use crate::units::{direction_to_godot, GodotPos, SkyrimPos};

/// How a shape wants to be shaded, as far as a first-pass
/// `StandardMaterial3D` can honour it. Texture paths are the NIF's own,
/// relative to `Data/`, and are resolved by the asset layer, not here.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDesc {
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub glow: Option<String>,
    pub emissive_color: [f32; 3],
    pub emissive_multiple: f32,
    pub environment: Option<String>,
    pub environment_mask: Option<String>,
    pub environment_scale: f32,
    /// BSLightingShaderProperty shader flags 1 and 2, verbatim. The mapping
    /// to material features happens in the Godot layer, where the feature
    /// set is known.
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    /// BSLightingShaderType 18 uses the shared terrain noise mask.
    pub landscape_lod_noise: bool,
    pub alpha: AlphaMode,
    /// Two-sided rendering, from the shader flags or an NiStencilProperty.
    pub double_sided: bool,
    /// WATR shading for cell water planes and placed BSWaterShaderProperty
    /// meshes whose ACTI base resolves a water type.
    pub water: Option<WaterMaterial>,
    /// BSEffectShaderProperty fields and its NiAlphaProperty contract.
    pub effect: Option<EffectMaterial>,
    /// Lighting refraction uses its texture as distortion, never visible color.
    pub refraction: Option<RefractionMaterial>,
    /// Lighting-shader UV coordinates and their authored float tracks.
    pub texture_transform: Option<TextureTransform>,
}

/// BSLightingShaderProperty screen distortion inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct RefractionMaterial {
    pub strength: f32,
    pub alpha: f32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub clamp_mode: u32,
}

impl RefractionMaterial {
    fn key(&self, flags: [u32; 2]) -> [u32; 9] {
        [
            self.strength.to_bits(),
            self.alpha.to_bits(),
            self.uv_offset[0].to_bits(),
            self.uv_offset[1].to_bits(),
            self.uv_scale[0].to_bits(),
            self.uv_scale[1].to_bits(),
            self.clamp_mode,
            flags[0],
            flags[1],
        ]
    }
}

/// Authored NIF effect shading, independent of the Godot renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectMaterial {
    pub params: soe_nif::mesh::EffectParams,
    pub blend: Option<soe_nif::mesh::Blend>,
    pub alpha_test: Option<soe_nif::mesh::AlphaTest>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Standalone authored effect float tracks, evaluated by the shader.
    pub controllers: Vec<crate::nif_effect_animation::EffectFloatTrack>,
}

/// Lighting texture coordinates shared by diffuse, normal and reflection masks.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureTransform {
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub controllers: Vec<crate::nif_effect_animation::EffectFloatTrack>,
}

impl TextureTransform {
    pub(crate) fn key(&self, flags: [u32; 2]) -> ([u32; 6], String) {
        (
            [
                self.offset[0].to_bits(),
                self.offset[1].to_bits(),
                self.scale[0].to_bits(),
                self.scale[1].to_bits(),
                flags[0],
                flags[1],
            ],
            crate::nif_effect_animation::identity(&self.controllers),
        )
    }
}

/// Bitwise material identity, including effect flags and palette paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectKey {
    palette: String,
    values: Vec<u32>,
    flags: [u32; 2],
    blend: Option<(u8, u8)>,
    alpha_test: Option<(u8, u32)>,
    clamp_mode: u8,
    controllers: String,
}

impl EffectMaterial {
    pub(crate) fn key(&self, flags: [u32; 2]) -> EffectKey {
        let p = &self.params;
        let values = p
            .base_color
            .into_iter()
            .chain([
                p.base_color_scale,
                p.falloff_start_angle,
                p.falloff_stop_angle,
                p.falloff_start_opacity,
                p.falloff_stop_opacity,
                p.soft_falloff_depth,
                p.lighting_influence,
                self.uv_offset[0],
                self.uv_offset[1],
                self.uv_scale[0],
                self.uv_scale[1],
            ])
            .map(f32::to_bits)
            .collect();
        EffectKey {
            palette: soe_bsa::normalize(&p.greyscale_texture),
            values,
            flags,
            blend: self.blend.map(|b| (b.src as u8, b.dst as u8)),
            alpha_test: self
                .alpha_test
                .map(|a| (a.func as u8, a.threshold.to_bits())),
            clamp_mode: p.clamp_mode,
            controllers: crate::nif_effect_animation::identity(&self.controllers),
        }
    }
}

impl Default for MaterialDesc {
    fn default() -> Self {
        Self {
            diffuse: None,
            normal: None,
            glow: None,
            emissive_color: [0.0; 3],
            emissive_multiple: 1.0,
            environment: None,
            environment_mask: None,
            environment_scale: 1.0,
            shader_flags_1: 0,
            shader_flags_2: 0,
            landscape_lod_noise: false,
            alpha: AlphaMode::default(),
            double_sided: false,
            water: None,
            effect: None,
            refraction: None,
            texture_transform: None,
        }
    }
}

/// Shading data read from a WATR record, carried by a water plane's
/// `MaterialDesc`, which `godot_layer::water_material` maps onto a Godot
/// material. Colors are normalised 0.0 to 1.0, `opacity` is the
/// record's own 0 to 255 byte.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WaterMaterial {
    pub opacity: u8,
    pub shallow_color: [f32; 3],
    pub deep_color: [f32; 3],
    pub reflection_color: [f32; 3],
    /// The first of WATR's three noise texture layers (NAM2).
    pub noise_texture: Option<String>,
    /// NAM2, NAM3 and NAM4, the three scrolling normal layers.
    pub noise_layers: [Option<String>; 3],
    /// NAM5, the flow normals texture used on BSWaterShaderProperty meshes.
    pub flow_texture: Option<String>,
    /// DNAM numeric shading constants, see `WaterShading`.
    pub shading: WaterShading,
    /// FNAM bits, including Enable Flowmap and Blend Normals.
    pub flags: u8,
    /// Authored NIF inputs, absent on generated cell planes.
    pub mesh: Option<WaterMesh>,
}

/// The DNAM constants the water shader reads. xEdit supplies scalar defaults.
/// Missing fog ranges and normal amplitudes use a visible surface fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterShading {
    /// DNAM 16, sun specular power.
    pub sun_specular_power: f32,
    /// DNAM 20, reflectivity amount.
    pub reflectivity: f32,
    /// DNAM 24, fresnel amount.
    pub fresnel: f32,
    /// DNAM 32 and 36, above water fog near and far plane in game units.
    pub fog_near: f32,
    pub fog_far: f32,
    /// DNAM 132, above water fog amount.
    pub fog_amount: f32,
    /// DNAM 100, 104 and 108 are wind direction per layer, 112 to 120 wind
    /// speed per layer. Layer scroll velocity is speed along direction.
    pub wind_direction: [f32; 3],
    pub wind_speed: [f32; 3],
    /// DNAM 172 to 180, UV scale per layer.
    pub uv_scale: [f32; 3],
    /// DNAM 184 to 192, amplitude scale per layer.
    pub amplitude_scale: [f32; 3],
    /// DNAM 196, reflection magnitude.
    pub reflection_magnitude: f32,
    /// DNAM 200, sun sparkle magnitude.
    pub sparkle_magnitude: f32,
    /// DNAM 204, sun specular magnitude.
    pub sun_specular_magnitude: f32,
    /// DNAM 224, sun sparkle power.
    pub sparkle_power: f32,
    /// DNAM 228, flow map scale.
    pub flowmap_scale: f32,
}

impl Default for WaterShading {
    fn default() -> Self {
        Self {
            sun_specular_power: 50.0,
            reflectivity: 0.5,
            fresnel: 0.02,
            fog_near: 0.0,
            fog_far: 1000.0,
            fog_amount: 1.0,
            wind_direction: [0.0; 3],
            wind_speed: [0.0; 3],
            uv_scale: [100.0; 3],
            amplitude_scale: [1.0; 3],
            reflection_magnitude: 1.0,
            sparkle_magnitude: 1.0,
            sun_specular_magnitude: 1.0,
            sparkle_power: 1.0,
            flowmap_scale: 1.0,
        }
    }
}

impl WaterShading {
    /// Texture-space scroll per second. Spatial noise scale affects coordinates,
    /// not the separate scroll term in Community Shaders Water.hlsl.
    pub fn normal_velocities(&self) -> [[f32; 2]; 3] {
        std::array::from_fn(|i| {
            let (sin, cos) = self.wind_direction[i].to_radians().sin_cos();
            [cos * self.wind_speed[i], sin * self.wind_speed[i]]
        })
    }
}

/// nif.xml's BSWaterShaderProperty inputs, retained at the host boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterMesh {
    pub flags1: u32,
    pub flags2: u32,
    pub water_flags: u32,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
}

/// Bitwise identity for all water shading, texture and mesh inputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WaterKey {
    opacity: u8,
    flags: u8,
    values: Vec<u32>,
    textures: [Option<String>; 3],
    flow_texture: Option<String>,
    mesh: Option<[u32; 7]>,
}

impl WaterMaterial {
    /// Opacity as the 0.0 to 1.0 alpha a blended material draws with.
    pub fn alpha(&self) -> f32 {
        f32::from(self.opacity) / 255.0
    }

    /// Normal path for a layer, retaining the original layer-one accessor.
    pub fn noise_layer(&self, layer: usize) -> Option<&str> {
        self.noise_layers
            .get(layer)
            .and_then(Option::as_deref)
            .or_else(|| {
                (layer == 0)
                    .then_some(self.noise_texture.as_deref())
                    .flatten()
            })
    }

    fn key(&self) -> WaterKey {
        let p = &self.shading;
        let values = self
            .shallow_color
            .into_iter()
            .chain(self.deep_color)
            .chain(self.reflection_color)
            .chain([
                p.sun_specular_power,
                p.reflectivity,
                p.fresnel,
                p.fog_near,
                p.fog_far,
                p.fog_amount,
                p.reflection_magnitude,
                p.sparkle_magnitude,
                p.sun_specular_magnitude,
                p.sparkle_power,
                p.flowmap_scale,
            ])
            .chain(p.wind_direction)
            .chain(p.wind_speed)
            .chain(p.uv_scale)
            .chain(p.amplitude_scale)
            .map(f32::to_bits)
            .collect();
        WaterKey {
            opacity: self.opacity,
            flags: self.flags,
            values,
            textures: std::array::from_fn(|i| self.noise_layer(i).map(soe_bsa::normalize)),
            flow_texture: self.flow_texture.as_deref().map(soe_bsa::normalize),
            mesh: self.mesh.as_ref().map(|m| {
                [
                    m.flags1,
                    m.flags2,
                    m.water_flags,
                    m.uv_offset[0].to_bits(),
                    m.uv_offset[1].to_bits(),
                    m.uv_scale[0].to_bits(),
                    m.uv_scale[1].to_bits(),
                ]
            }),
        }
    }
}

impl MaterialDesc {
    /// Lighting buffers retain the enabled RGB and alpha channels, neutralizing others.
    pub fn uses_vertex_colors(&self) -> bool {
        self.shader_flags_2 & soe_nif::blocks::flags2::TREE_ANIM == 0
            && (self.shader_flags_1 & soe_nif::blocks::flags1::VERTEX_ALPHA != 0
                || self.shader_flags_2 & soe_nif::blocks::flags2::VERTEX_COLORS != 0)
    }

    /// Landscape LOD stores model-space normals with RGB encoding X, Z, Y.
    pub fn landscape_lod_normals(&self) -> bool {
        self.shader_flags_1 & soe_nif::blocks::flags1::MODEL_SPACE_NORMALS != 0
            && self.shader_flags_2 & soe_nif::blocks::flags2::LOD_LANDSCAPE != 0
    }

    /// The cubemap slot is active only for authored environment mapping.
    pub fn environment_texture(&self) -> Option<&str> {
        (self.shader_flags_1 & soe_nif::blocks::flags1::ENVIRONMENT_MAPPING != 0)
            .then_some(self.environment.as_deref())
            .flatten()
    }

    /// Slot two is a glow map with SLSF2_Glow_Map. Some Own_Emit candle
    /// materials omit that flag but name their emission texture with `_g`.
    pub fn glow_texture(&self) -> Option<&str> {
        let glow_flag = self.shader_flags_2 & soe_nif::blocks::flags2::GLOW_MAP != 0;
        let own_emit_texture = self.shader_flags_1 & soe_nif::blocks::flags1::OWN_EMIT != 0
            && self
                .glow
                .as_deref()
                .is_some_and(|path| path.to_ascii_lowercase().ends_with("_g.dds"));
        (glow_flag || own_emit_texture)
            .then_some(self.glow.as_deref())
            .flatten()
    }

    pub fn emits_light(&self) -> bool {
        let emission_flag = self.shader_flags_1 & soe_nif::blocks::flags1::OWN_EMIT != 0
            || self.shader_flags_2 & soe_nif::blocks::flags2::GLOW_MAP != 0;
        emission_flag
            && self.emissive_multiple > 0.0
            && self.emissive_color.iter().any(|channel| *channel > 0.0)
    }

    /// Decals need a depth offset, with their authored depth test/write flags.
    pub fn decal_depth(&self) -> Option<(bool, bool)> {
        use soe_nif::blocks::{flags1, flags2};
        (self.shader_flags_1 & flags1::DECAL != 0).then_some((
            self.shader_flags_1 & flags1::ZBUFFER_TEST != 0,
            self.shader_flags_2 & flags2::ZBUFFER_WRITE != 0,
        ))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    #[default]
    Opaque,
    /// Alpha test against a threshold in 0 to 255.
    Cutout(u8),
    Blend,
}

/// Every feature consumed by the host's material builders.
/// Texture upload identity is separate from alpha and culling state.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MaterialKey {
    vertex_colors: bool,
    pub environment: Option<String>,
    pub environment_mask: Option<String>,
    pub environment_scale: u32,
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub glow: Option<String>,
    pub emissive_color: [u32; 3],
    pub emissive_multiple: u32,
    tree_animated: bool,
    alpha: AlphaMode,
    double_sided: bool,
    decal_depth: Option<(bool, bool)>,
    /// All water constants and texture paths, including authored mesh inputs.
    water: Option<WaterKey>,
    effect: Option<EffectKey>,
    refraction: Option<[u32; 9]>,
    texture_transform: Option<([u32; 6], String)>,
    landscape_lod_normals: bool,
    landscape_lod_noise: bool,
}

impl From<&MaterialDesc> for MaterialKey {
    fn from(material: &MaterialDesc) -> Self {
        Self {
            vertex_colors: material.uses_vertex_colors(),
            environment: material.environment_texture().map(str::to_lowercase),
            environment_mask: material.environment_mask.as_deref().map(str::to_lowercase),
            environment_scale: material.environment_scale.to_bits(),
            diffuse: material.diffuse.as_deref().map(str::to_lowercase),
            normal: material.normal.as_deref().map(str::to_lowercase),
            glow: material.glow_texture().map(str::to_lowercase),
            emissive_color: material.emissive_color.map(f32::to_bits),
            emissive_multiple: material.emissive_multiple.to_bits(),
            tree_animated: material.shader_flags_2 & soe_nif::blocks::flags2::TREE_ANIM != 0,
            alpha: material.alpha,
            double_sided: material.double_sided,
            decal_depth: material.decal_depth(),
            water: material.water.as_ref().map(WaterMaterial::key),
            effect: material
                .effect
                .as_ref()
                .map(|e| e.key([material.shader_flags_1, material.shader_flags_2])),
            refraction: material
                .refraction
                .as_ref()
                .map(|r| r.key([material.shader_flags_1, material.shader_flags_2])),
            texture_transform: material
                .texture_transform
                .as_ref()
                .map(|t| t.key([material.shader_flags_1, material.shader_flags_2])),
            landscape_lod_normals: material.landscape_lod_normals(),
            landscape_lod_noise: material.landscape_lod_normals() && material.landscape_lod_noise,
        }
    }
}

/// What a NIF shape provides. Slices are per vertex except `indices`, which
/// is a triangle list. Bone indices refer to the shape's own bone list.
pub trait MeshSource {
    fn positions(&self) -> &[[f32; 3]];
    fn normals(&self) -> Option<&[[f32; 3]]>;
    /// xyz plus handedness sign in w, as NIF bitangent sign.
    fn tangents(&self) -> Option<&[[f32; 4]]>;
    fn uvs(&self) -> Option<&[[f32; 2]]>;
    fn colors(&self) -> Option<&[[f32; 4]]> {
        None
    }
    fn indices(&self) -> &[u32];
    fn bone_indices(&self) -> Option<&[[u16; 4]]>;
    fn bone_weights(&self) -> Option<&[[f32; 4]]>;
    fn material(&self) -> &MaterialDesc;
}

/// An owned source, for tests and for callers that decode into it directly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub tangents: Option<Vec<[f32; 4]>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub colors: Option<Vec<[f32; 4]>>,
    pub indices: Vec<u32>,
    pub bone_indices: Option<Vec<[u16; 4]>>,
    pub bone_weights: Option<Vec<[f32; 4]>>,
    pub material: MaterialDesc,
}

impl MeshSource for MeshData {
    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
    fn normals(&self) -> Option<&[[f32; 3]]> {
        self.normals.as_deref()
    }
    fn tangents(&self) -> Option<&[[f32; 4]]> {
        self.tangents.as_deref()
    }
    fn uvs(&self) -> Option<&[[f32; 2]]> {
        self.uvs.as_deref()
    }
    fn colors(&self) -> Option<&[[f32; 4]]> {
        self.colors.as_deref()
    }
    fn indices(&self) -> &[u32] {
        &self.indices
    }
    fn bone_indices(&self) -> Option<&[[u16; 4]]> {
        self.bone_indices.as_deref()
    }
    fn bone_weights(&self) -> Option<&[[f32; 4]]> {
        self.bone_weights.as_deref()
    }
    fn material(&self) -> &MaterialDesc {
        &self.material
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshOptions {
    /// Reverse each triangle. Godot treats clockwise as front-facing. Whether
    /// Skyrim's shapes come out counter-clockwise after the axis map is
    /// unverified until a real NIF renders, so this is a switch and not a
    /// constant. Default on.
    pub flip_winding: bool,
    /// Compute flat-ish vertex normals when the source has none.
    pub generate_missing_normals: bool,
}

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            flip_winding: true,
            generate_missing_normals: true,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MeshError {
    #[error("shape has no vertices")]
    Empty,
    #[error("index count {0} is not a multiple of three")]
    NotTriangles(usize),
    #[error("index {index} at position {at} exceeds vertex count {vertices}")]
    IndexOutOfRange {
        index: u32,
        at: usize,
        vertices: usize,
    },
    #[error("{attribute} has {got} entries for {vertices} vertices")]
    AttributeLength {
        attribute: &'static str,
        got: usize,
        vertices: usize,
    },
    #[error("bone indices without weights, or weights without indices")]
    HalfSkinned,
}

/// Axis-aligned bounds in Godot metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: GodotPos,
    pub max: GodotPos,
}

/// One `ArrayMesh` surface in Godot's conventions: metres, Y up, tangents
/// as four floats per vertex, bones and weights four per vertex, flat.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceArrays {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<f32>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<i32>,
    pub bones: Vec<i32>,
    pub weights: Vec<f32>,
    pub material: MaterialDesc,
    pub bounds: Aabb,
}

impl SurfaceArrays {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bytes in the vertex and index arrays submitted for mesh upload.
    pub fn mesh_byte_count(&self) -> usize {
        self.positions.len() * std::mem::size_of::<[f32; 3]>()
            + self.normals.len() * std::mem::size_of::<[f32; 3]>()
            + self.tangents.len() * std::mem::size_of::<f32>()
            + self.uvs.len() * std::mem::size_of::<[f32; 2]>()
            + self.colors.len() * std::mem::size_of::<[f32; 4]>()
            + self.indices.len() * std::mem::size_of::<i32>()
            + self.bones.len() * std::mem::size_of::<i32>()
            + self.weights.len() * std::mem::size_of::<f32>()
    }

    #[cfg(feature = "godot")]
    pub(crate) fn allocation_bytes(&self) -> usize {
        self.positions.capacity() * std::mem::size_of::<[f32; 3]>()
            + self.normals.capacity() * std::mem::size_of::<[f32; 3]>()
            + self.tangents.capacity() * std::mem::size_of::<f32>()
            + self.uvs.capacity() * std::mem::size_of::<[f32; 2]>()
            + self.colors.capacity() * std::mem::size_of::<[f32; 4]>()
            + self.indices.capacity() * std::mem::size_of::<i32>()
            + self.bones.capacity() * std::mem::size_of::<i32>()
            + self.weights.capacity() * std::mem::size_of::<f32>()
    }

    pub fn is_skinned(&self) -> bool {
        !self.bones.is_empty()
    }
}

fn check_len(attribute: &'static str, got: usize, vertices: usize) -> Result<(), MeshError> {
    if got == vertices {
        Ok(())
    } else {
        Err(MeshError::AttributeLength {
            attribute,
            got,
            vertices,
        })
    }
}

fn validate(src: &dyn MeshSource) -> Result<(), MeshError> {
    let n = src.positions().len();
    if n == 0 {
        return Err(MeshError::Empty);
    }
    let idx = src.indices();
    if !idx.len().is_multiple_of(3) {
        return Err(MeshError::NotTriangles(idx.len()));
    }
    for (at, &i) in idx.iter().enumerate() {
        if i as usize >= n {
            return Err(MeshError::IndexOutOfRange {
                index: i,
                at,
                vertices: n,
            });
        }
    }
    if let Some(v) = src.colors() {
        check_len("colors", v.len(), n)?;
    }
    if let Some(v) = src.normals() {
        check_len("normals", v.len(), n)?;
    }
    if let Some(v) = src.tangents() {
        check_len("tangents", v.len(), n)?;
    }
    if let Some(v) = src.uvs() {
        check_len("uvs", v.len(), n)?;
    }
    match (src.bone_indices(), src.bone_weights()) {
        (Some(b), Some(w)) => {
            check_len("bone indices", b.len(), n)?;
            check_len("bone weights", w.len(), n)?;
        }
        (None, None) => {}
        _ => return Err(MeshError::HalfSkinned),
    }
    Ok(())
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-12 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Area-weighted vertex normals from the triangle list, in the already
/// converted Godot positions and winding.
fn generate_normals(positions: &[[f32; 3]], indices: &[i32]) -> Vec<[f32; 3]> {
    let mut acc = vec![[0.0f32; 3]; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            positions[tri[0] as usize],
            positions[tri[1] as usize],
            positions[tri[2] as usize],
        );
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        // Godot front faces are clockwise when seen from outside, so the
        // outward normal is ac x ab, the reverse of the usual right-hand rule.
        let n = [
            ac[1] * ab[2] - ac[2] * ab[1],
            ac[2] * ab[0] - ac[0] * ab[2],
            ac[0] * ab[1] - ac[1] * ab[0],
        ];
        for &i in tri {
            let v = &mut acc[i as usize];
            v[0] += n[0];
            v[1] += n[1];
            v[2] += n[2];
        }
    }
    acc.into_iter().map(normalize).collect()
}

/// Build one surface. Fails rather than producing arrays Godot would reject
/// at `add_surface_from_arrays`, where the error is a console line and a
/// missing mesh.
pub fn build_surface(src: &dyn MeshSource, opts: MeshOptions) -> Result<SurfaceArrays, MeshError> {
    validate(src)?;

    let positions: Vec<[f32; 3]> = src
        .positions()
        .iter()
        .map(|p| {
            let g = SkyrimPos::new(p[0], p[1], p[2]).to_godot();
            [g.x, g.y, g.z]
        })
        .collect();

    let mut indices: Vec<i32> = src.indices().iter().map(|&i| i as i32).collect();
    if opts.flip_winding {
        for tri in indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }

    let normals = match src.normals() {
        Some(n) => n
            .iter()
            .map(|v| normalize(direction_to_godot(*v)))
            .collect(),
        None if opts.generate_missing_normals => generate_normals(&positions, &indices),
        None => Vec::new(),
    };

    let tangents = match src.tangents() {
        Some(t) => t
            .iter()
            .flat_map(|v| {
                let d = normalize(direction_to_godot([v[0], v[1], v[2]]));
                [d[0], d[1], d[2], if v[3] < 0.0 { -1.0 } else { 1.0 }]
            })
            .collect(),
        None => Vec::new(),
    };

    let uvs = src.uvs().map(|u| u.to_vec()).unwrap_or_default();

    let (bones, weights) = match (src.bone_indices(), src.bone_weights()) {
        (Some(b), Some(w)) => (
            b.iter().flat_map(|v| v.map(|i| i as i32)).collect(),
            w.iter().flat_map(|v| *v).collect(),
        ),
        _ => (Vec::new(), Vec::new()),
    };

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in &positions {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }

    Ok(SurfaceArrays {
        positions,
        normals,
        tangents,
        uvs,
        colors: src.colors().map(<[_]>::to_vec).unwrap_or_default(),
        indices,
        bones,
        weights,
        material: src.material().clone(),
        bounds: Aabb {
            min: GodotPos::new(min[0], min[1], min[2]),
            max: GodotPos::new(max[0], max[1], max[2]),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::METRES_PER_UNIT;

    /// A unit right triangle in the Skyrim XY plane, facing up (+Z), wound
    /// counter-clockwise seen from above.
    fn floor_triangle() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [70.0, 0.0, 0.0], [0.0, 70.0, 0.0]],
            indices: vec![0, 1, 2],
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
            ..MeshData::default()
        }
    }

    #[test]
    fn positions_are_converted_to_metres_y_up() {
        let s = build_surface(&floor_triangle(), MeshOptions::default()).unwrap();
        assert!((s.positions[1][0] - 70.0 * METRES_PER_UNIT).abs() < 1e-5);
        // Skyrim +Y becomes Godot -Z.
        assert!((s.positions[2][2] + 70.0 * METRES_PER_UNIT).abs() < 1e-5);
        assert_eq!(s.positions[2][1], 0.0);
        assert_eq!(s.uvs.len(), 3);
        assert_eq!(s.triangle_count(), 1);
        assert!(!s.is_skinned());
    }

    #[test]
    fn flipping_swaps_the_second_and_third_index_of_each_triangle() {
        let flipped = build_surface(&floor_triangle(), MeshOptions::default()).unwrap();
        assert_eq!(flipped.indices, vec![0, 2, 1]);
        let kept = build_surface(
            &floor_triangle(),
            MeshOptions {
                flip_winding: false,
                ..MeshOptions::default()
            },
        )
        .unwrap();
        assert_eq!(kept.indices, vec![0, 1, 2]);
    }

    #[test]
    fn generated_normals_point_up_for_a_floor_after_the_flip() {
        let s = build_surface(&floor_triangle(), MeshOptions::default()).unwrap();
        for n in &s.normals {
            assert!((n[1] - 1.0).abs() < 1e-5, "normal {n:?} should be +Y");
        }
    }

    #[test]
    fn source_normals_and_tangents_are_remapped_and_normalised() {
        let mut m = floor_triangle();
        m.normals = Some(vec![[0.0, 0.0, 2.0]; 3]);
        m.tangents = Some(vec![[0.0, 3.0, 0.0, -1.0]; 3]);
        let s = build_surface(&m, MeshOptions::default()).unwrap();
        assert_eq!(s.normals[0], [0.0, 1.0, 0.0]);
        assert_eq!(&s.tangents[0..4], &[0.0, 0.0, -1.0, -1.0]);
    }

    #[test]
    fn skinning_is_flattened_four_per_vertex() {
        let mut m = floor_triangle();
        m.bone_indices = Some(vec![[1, 0, 0, 0]; 3]);
        m.bone_weights = Some(vec![[1.0, 0.0, 0.0, 0.0]; 3]);
        let s = build_surface(&m, MeshOptions::default()).unwrap();
        assert!(s.is_skinned());
        assert_eq!(s.bones.len(), 12);
        assert_eq!(s.weights.len(), 12);
        assert_eq!(s.bones[0], 1);
    }

    #[test]
    fn bounds_cover_every_vertex() {
        let s = build_surface(&floor_triangle(), MeshOptions::default()).unwrap();
        assert_eq!(
            s.bounds.min,
            GodotPos::new(0.0, 0.0, -70.0 * METRES_PER_UNIT)
        );
        assert!((s.bounds.max.x - 70.0 * METRES_PER_UNIT).abs() < 1e-6);
        assert_eq!(s.bounds.max.z, 0.0);
    }

    #[test]
    fn malformed_sources_are_refused_with_the_reason() {
        let empty = MeshData::default();
        assert_eq!(
            build_surface(&empty, MeshOptions::default()).err(),
            Some(MeshError::Empty)
        );

        let mut m = floor_triangle();
        m.indices = vec![0, 1];
        assert_eq!(
            build_surface(&m, MeshOptions::default()).err(),
            Some(MeshError::NotTriangles(2))
        );

        let mut m = floor_triangle();
        m.indices = vec![0, 1, 7];
        assert_eq!(
            build_surface(&m, MeshOptions::default()).err(),
            Some(MeshError::IndexOutOfRange {
                index: 7,
                at: 2,
                vertices: 3
            })
        );

        let mut m = floor_triangle();
        m.uvs = Some(vec![[0.0, 0.0]]);
        assert_eq!(
            build_surface(&m, MeshOptions::default()).err(),
            Some(MeshError::AttributeLength {
                attribute: "uvs",
                got: 1,
                vertices: 3
            })
        );

        let mut m = floor_triangle();
        m.bone_weights = Some(vec![[1.0, 0.0, 0.0, 0.0]; 3]);
        assert_eq!(
            build_surface(&m, MeshOptions::default()).err(),
            Some(MeshError::HalfSkinned)
        );
    }
}

#[cfg(test)]
mod material_key_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn decals_keep_depth_state_separate_from_plain_materials() {
        use soe_nif::blocks::{flags1, flags2};
        let plain = MaterialDesc::default();
        let mut keys = std::collections::HashSet::from([MaterialKey::from(&plain)]);
        for test in [0, flags1::ZBUFFER_TEST] {
            for write in [0, flags2::ZBUFFER_WRITE] {
                let decal = MaterialDesc {
                    shader_flags_1: flags1::DECAL | test,
                    shader_flags_2: write,
                    ..plain.clone()
                };
                assert_eq!(decal.decal_depth(), Some((test != 0, write != 0)));
                assert!(keys.insert(MaterialKey::from(&decal)));
            }
        }
        assert_eq!(keys.len(), 5);
    }

    #[test]
    fn rnd_02_each_water_type_keeps_its_own_material() {
        let plain = MaterialDesc::default();
        let water = |opacity, shallow| MaterialDesc {
            water: Some(WaterMaterial {
                opacity,
                shallow_color: shallow,
                ..WaterMaterial::default()
            }),
            ..MaterialDesc::default()
        };
        let keys = std::collections::HashSet::from([
            MaterialKey::from(&plain),
            MaterialKey::from(&water(255, [0.1, 0.2, 0.3])),
            MaterialKey::from(&water(128, [0.1, 0.2, 0.3])),
            MaterialKey::from(&water(128, [0.1, 0.2, 0.4])),
        ]);
        assert_eq!(keys.len(), 4);
        assert_eq!(
            MaterialKey::from(&water(128, [0.1, 0.2, 0.3])),
            MaterialKey::from(&water(128, [0.1, 0.2, 0.3]))
        );
        assert!(
            (WaterMaterial {
                opacity: 51,
                ..Default::default()
            }
            .alpha()
                - 0.2)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn water_shader_wind_rates_do_not_slow_down_with_larger_noise_tiles() {
        let shading = WaterShading {
            wind_direction: [0.0, 90.0, 180.0],
            wind_speed: [0.203, 0.065, 0.058],
            uv_scale: [942.0, 2101.0, 3116.0],
            ..WaterShading::default()
        };
        let velocities = shading.normal_velocities();
        for (velocity, expected) in
            velocities
                .iter()
                .zip([[0.203, 0.0], [0.0, 0.065], [-0.058, 0.0]])
        {
            assert!((velocity[0] - expected[0]).abs() < 0.000001);
            assert!((velocity[1] - expected[1]).abs() < 0.000001);
            assert!(
                velocity[0].hypot(velocity[1]) * 0.5 > 0.02,
                "all layers must move at least 0.02 UV in half a second"
            );
        }
        let smaller_tiles = WaterShading {
            uv_scale: [1.0; 3],
            ..shading
        };
        assert_eq!(velocities, smaller_tiles.normal_velocities());
    }

    #[test]
    fn water_shader_legacy_layer_one_path_has_the_same_cache_identity() {
        let legacy = WaterMaterial {
            noise_texture: Some("Textures\\Water\\Noise.dds".into()),
            ..WaterMaterial::default()
        };
        let layers = WaterMaterial {
            noise_layers: [Some("textures/water/noise.dds".into()), None, None],
            ..WaterMaterial::default()
        };
        assert_eq!(legacy.noise_layer(0), legacy.noise_texture.as_deref());
        assert_eq!(legacy.key(), layers.key());
        assert_eq!(legacy.noise_layer(3), None);
    }

    #[test]
    fn water_shader_cache_key_covers_shading_textures_and_mesh_inputs() {
        let desc = |water| MaterialDesc {
            water: Some(water),
            ..MaterialDesc::default()
        };
        let base = WaterMaterial::default();
        let key = MaterialKey::from(&desc(base.clone()));
        let mut variants = Vec::new();
        for i in 0..3 {
            let mut material = base.clone();
            material.noise_layers[i] = Some(format!("water/{i}.dds"));
            variants.push(material);
            for kind in 0..4 {
                let mut material = base.clone();
                match kind {
                    0 => material.shading.wind_direction[i] += 1.0,
                    1 => material.shading.wind_speed[i] += 1.0,
                    2 => material.shading.uv_scale[i] += 1.0,
                    _ => material.shading.amplitude_scale[i] += 1.0,
                }
                variants.push(material);
            }
        }
        for kind in 0..13 {
            let mut material = base.clone();
            match kind {
                0 => material.shading.sun_specular_power += 1.0,
                1 => material.shading.reflectivity += 1.0,
                2 => material.shading.fresnel += 1.0,
                3 => material.shading.fog_near += 1.0,
                4 => material.shading.fog_far += 1.0,
                5 => material.shading.fog_amount += 1.0,
                6 => material.shading.reflection_magnitude += 1.0,
                7 => material.shading.sparkle_magnitude += 1.0,
                8 => material.shading.sun_specular_magnitude += 1.0,
                9 => material.shading.sparkle_power += 1.0,
                10 => material.shading.flowmap_scale += 1.0,
                11 => material.flags = 8,
                _ => material.flow_texture = Some("flow.dds".into()),
            }
            variants.push(material);
        }
        let mesh = WaterMesh {
            flags1: 8,
            flags2: 0x21,
            water_flags: 0x1C4,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
        };
        let mut material = base.clone();
        material.mesh = Some(mesh.clone());
        let mesh_key = MaterialKey::from(&desc(material.clone()));
        variants.push(material);
        for kind in 0..7 {
            let mut changed = mesh.clone();
            match kind {
                0 => changed.flags1 ^= 1,
                1 => changed.flags2 ^= 1,
                2 => changed.water_flags ^= 1,
                3 => changed.uv_offset[0] += 1.0,
                4 => changed.uv_offset[1] += 1.0,
                5 => changed.uv_scale[0] += 1.0,
                _ => changed.uv_scale[1] += 1.0,
            }
            let mut material = base.clone();
            material.mesh = Some(changed);
            assert_ne!(mesh_key, MaterialKey::from(&desc(material.clone())));
            variants.push(material);
        }
        for variant in variants {
            assert_ne!(key, MaterialKey::from(&desc(variant)));
        }
    }

    #[test]
    fn emission_color_and_glow_path_keep_distinct_materials() {
        let base = MaterialDesc {
            glow: Some("Textures\\Glow.dds".into()),
            emissive_color: [1.0, 0.0, 0.0],
            shader_flags_2: soe_nif::blocks::flags2::GLOW_MAP,
            ..Default::default()
        };
        let mut other_color = base.clone();
        other_color.emissive_color = [0.0, 1.0, 0.0];
        let mut other_map = base.clone();
        other_map.glow = Some("textures\\other.dds".into());
        assert_ne!(MaterialKey::from(&base), MaterialKey::from(&other_color));
        assert_ne!(MaterialKey::from(&base), MaterialKey::from(&other_map));
        let mut lower = base.clone();
        lower.glow = lower.glow.map(|path| path.to_lowercase());
        assert_eq!(MaterialKey::from(&base), MaterialKey::from(&lower));
    }

    #[test]
    fn rnd_24_emission_requires_own_emit_or_glow_map_and_glow_uses_slot_two() {
        use soe_nif::blocks::{flags1, flags2};

        let authored = MaterialDesc {
            emissive_color: [1.0, 0.5, 0.25],
            emissive_multiple: 2.0,
            ..Default::default()
        };
        assert!(!authored.emits_light());
        assert_eq!(authored.glow_texture(), None);

        let own_emit = MaterialDesc {
            shader_flags_1: flags1::OWN_EMIT,
            ..authored.clone()
        };
        assert!(own_emit.emits_light());
        assert_eq!(own_emit.glow_texture(), None);

        let glow_map = MaterialDesc {
            glow: Some("textures\\candle_g.dds".into()),
            shader_flags_2: flags2::GLOW_MAP,
            ..authored.clone()
        };
        assert!(glow_map.emits_light());
        assert_eq!(glow_map.glow_texture(), Some("textures\\candle_g.dds"));

        let own_emit_texture = MaterialDesc {
            glow: Some("textures\\candles_g.dds".into()),
            shader_flags_1: flags1::OWN_EMIT,
            ..authored.clone()
        };
        assert!(own_emit_texture.emits_light());
        assert_eq!(
            own_emit_texture.glow_texture(),
            Some("textures\\candles_g.dds")
        );

        let unused_slot = MaterialDesc {
            glow: Some("textures\\subsurface.dds".into()),
            ..authored
        };
        let no_glow_flag = MaterialDesc {
            shader_flags_1: flags1::OWN_EMIT,
            shader_flags_2: 0,
            ..unused_slot
        };
        assert_eq!(no_glow_flag.glow_texture(), None);
        assert!(no_glow_flag.emits_light(), "SLSF1_Own_Emit remains active");
    }

    #[test]
    fn shared_textures_keep_distinct_alpha_and_culling_in_both_orders() {
        let variants: Vec<_> = [
            AlphaMode::Opaque,
            AlphaMode::Cutout(64),
            AlphaMode::Cutout(128),
            AlphaMode::Blend,
        ]
        .into_iter()
        .flat_map(|alpha| {
            [false, true].map(|double_sided| MaterialDesc {
                diffuse: Some("Textures\\Leaf.dds".into()),
                normal: Some("Textures\\Leaf_n.dds".into()),
                alpha,
                double_sided,
                ..Default::default()
            })
        })
        .collect();
        for reverse in [false, true] {
            let mut order = variants.clone();
            if reverse {
                order.reverse();
            }
            let mut cache = HashMap::new();
            for desc in &order {
                cache.insert(MaterialKey::from(desc), desc.clone());
            }
            assert_eq!(cache.len(), variants.len());
            for desc in &variants {
                assert_eq!(cache.get(&MaterialKey::from(desc)), Some(desc));
                let mut lower = desc.clone();
                lower.diffuse = lower.diffuse.map(|s| s.to_lowercase());
                assert_eq!(MaterialKey::from(desc), MaterialKey::from(&lower));
            }
        }
    }
}
