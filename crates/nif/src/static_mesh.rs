// SPDX-License-Identifier: GPL-3.0-or-later

//! Import the static glTF subset used by the editor and export it as Skyrim SE
//! BSTriShape NIF geometry with BSLightingShaderProperty material links.

use std::path::Path;

use crate::blocks::{flags1, flags2, ShaderType};
use crate::error::{Error, Result};
use crate::mesh::{Material, MaterialKind, Mesh};
use crate::types::{Bound, Transform, Vec3};
use crate::vertex::{attr, VertexDesc};

const BS_SE: u32 = 100;
const NIF_VERSION: u32 = 0x1402_0007;

/// Imports triangle primitives from the default glTF scene as unskinned meshes.
/// Positions, normals and UVs are copied into renderer-neutral meshes. Node
/// transforms are baked into positions and normals. Texture links are kept as
/// paths from external image URIs.
pub fn import_gltf(path: impl AsRef<Path>) -> Result<Vec<Mesh>> {
    let path = path.as_ref();
    let gltf = gltf::Gltf::open(path).map_err(|error| invalid(error.to_string()))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let buffers = gltf
        .document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Uri(uri) if !uri.starts_with("data:") => {
                std::fs::read(base.join(uri)).map_err(|error| invalid(error.to_string()))
            }
            gltf::buffer::Source::Uri(_) => Err(invalid(
                "embedded glTF buffers are outside the static mesh subset",
            )),
            gltf::buffer::Source::Bin => gltf
                .blob
                .clone()
                .ok_or_else(|| invalid("glTF buffer has no binary payload")),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut meshes = Vec::new();
    let scene = gltf
        .document
        .default_scene()
        .or_else(|| gltf.document.scenes().next())
        .ok_or_else(|| invalid("glTF contains no scene"))?;
    for node in scene.nodes() {
        import_node(&node, identity(), base, &buffers, &mut meshes)?;
    }
    if meshes.is_empty() {
        return Err(invalid("glTF scene contains no static triangle primitives"));
    }
    Ok(meshes)
}

fn import_node(
    node: &gltf::Node<'_>,
    parent: [[f32; 4]; 4],
    base: &Path,
    buffers: &[Vec<u8>],
    output: &mut Vec<Mesh>,
) -> Result<()> {
    let transform = multiply(parent, node.transform().matrix());
    if node.skin().is_some() {
        return Err(invalid(
            "skinned glTF nodes are outside the static mesh subset",
        ));
    }
    if let Some(gltf_mesh) = node.mesh() {
        for (primitive_index, primitive) in gltf_mesh.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().next().is_some()
            {
                return Err(invalid("only non-morph triangle primitives are supported"));
            }
            if primitive.attributes().any(|(semantic, _)| {
                !matches!(
                    semantic,
                    gltf::Semantic::Positions
                        | gltf::Semantic::Normals
                        | gltf::Semantic::TexCoords(0)
                )
            }) {
                return Err(invalid(
                    "only POSITION, NORMAL, and TEXCOORD_0 attributes are supported",
                ));
            }
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let positions = reader
                .read_positions()
                .ok_or_else(|| invalid("primitive has no POSITION attribute"))?;
            let mut positions: Vec<[f32; 3]> = positions
                .map(|point| point3(transform_point(transform, point)))
                .collect();
            let mut normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|values| {
                    values
                        .map(|n| normalize(transform_normal(transform, n)))
                        .collect()
                })
                .unwrap_or_default();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|values| values.into_f32().map(|uv| [uv[0], uv[1]]).collect())
                .unwrap_or_default();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|values| values.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            validate_geometry(&positions, &normals, &uvs, &indices)?;
            if normals.is_empty() {
                normals = generate_normals(&positions, &indices);
            }
            let material = import_material(&primitive.material(), base)?;
            let name = node
                .name()
                .or_else(|| gltf_mesh.name())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Mesh{}Primitive{primitive_index}", gltf_mesh.index()));
            output.push(Mesh {
                name,
                block: 0,
                transform: Transform::default(),
                bound: bound(&positions),
                positions: std::mem::take(&mut positions),
                normals,
                tangents: Vec::new(),
                uvs,
                colors: Vec::new(),
                indices,
                bone_weights: Vec::new(),
                bone_indices: Vec::new(),
                skin: None,
                material,
            });
        }
    }
    for child in node.children() {
        import_node(&child, transform, base, buffers, output)?;
    }
    Ok(())
}

fn import_material(material: &gltf::Material<'_>, base: &Path) -> Result<Material> {
    let mut result = Material {
        kind: MaterialKind::Lighting(ShaderType::Default),
        ..Material::default()
    };
    let mut textures = vec![String::new(); 8];
    let pbr = material.pbr_metallic_roughness();
    if let Some(info) = pbr.base_color_texture() {
        textures[0] = texture_path(&info.texture(), base)?;
    }
    if let Some(info) = material.normal_texture() {
        textures[1] = texture_path(&info.texture(), base)?;
    }
    if let Some(info) = material.emissive_texture() {
        textures[2] = texture_path(&info.texture(), base)?;
    }
    if let Some(info) = pbr.metallic_roughness_texture() {
        textures[7] = texture_path(&info.texture(), base)?;
    }
    result.textures = textures;
    result.double_sided = material.double_sided();
    result.flags1 = flags1::CAST_SHADOWS | flags1::RECEIVE_SHADOWS | flags1::ZBUFFER_TEST;
    result.flags2 = flags2::ZBUFFER_WRITE
        | if result.double_sided {
            flags2::DOUBLE_SIDED
        } else {
            0
        };
    Ok(result)
}

fn texture_path(texture: &gltf::Texture<'_>, base: &Path) -> Result<String> {
    let gltf::image::Source::Uri { uri, .. } = texture.source().source() else {
        return Err(invalid(
            "embedded glTF images have no external material path",
        ));
    };
    let image_path = base.join(uri);
    let normalized = image_path
        .strip_prefix(base)
        .unwrap_or(&image_path)
        .to_string_lossy()
        .replace('/', "\\");
    Ok(normalized)
}

/// Writes a static Skyrim SE NIF containing one root and one BSTriShape per
/// supplied mesh. The supported material links are diffuse, normal, glow,
/// and specular texture slots in `BSShaderTextureSet`.
pub fn export_nif(meshes: &[Mesh]) -> Result<Vec<u8>> {
    if meshes.is_empty() {
        return Err(invalid("at least one mesh is required"));
    }
    let mut writer = NifWriter::new();
    let mut shape_refs = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        validate_mesh(mesh)?;
        let texture_ref = writer.texture_set(&mesh.material.textures);
        let shader_ref = writer.shader(texture_ref, &mesh.material);
        shape_refs.push(writer.shape(mesh, shader_ref));
    }
    let root = writer.node("Scene Root", &shape_refs);
    writer.finish(root)
}

fn validate_mesh(mesh: &Mesh) -> Result<()> {
    validate_geometry(&mesh.positions, &mesh.normals, &mesh.uvs, &mesh.indices)?;
    if mesh.positions.len() > usize::from(u16::MAX) {
        return Err(invalid("BSTriShape supports at most 65535 vertices"));
    }
    if mesh.indices.len() / 3 > usize::from(u16::MAX) {
        return Err(invalid("BSTriShape supports at most 65535 triangles"));
    }
    if mesh.indices.iter().any(|&index| index > u16::MAX as u32) {
        return Err(invalid("BSTriShape indices must fit in 16 bits"));
    }
    if !mesh.bone_weights.is_empty() || !mesh.bone_indices.is_empty() || mesh.skin.is_some() {
        return Err(invalid("skinned meshes are outside the static mesh subset"));
    }
    Ok(())
}

fn validate_geometry(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    indices: &[u32],
) -> Result<()> {
    if positions.is_empty() || indices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(invalid("geometry needs vertices and a triangle index list"));
    }
    if (!normals.is_empty() && normals.len() != positions.len())
        || (!uvs.is_empty() && uvs.len() != positions.len())
    {
        return Err(invalid("vertex attribute count does not match positions"));
    }
    if indices
        .iter()
        .any(|&index| index as usize >= positions.len())
    {
        return Err(invalid("triangle index exceeds the vertex count"));
    }
    if positions
        .iter()
        .flatten()
        .chain(normals.iter().flatten())
        .chain(uvs.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(invalid("geometry contains a non-finite attribute"));
    }
    Ok(())
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::InvalidData {
        block: 0,
        reason: reason.into(),
    }
}

fn identity() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn multiply(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            for k in 0..4 {
                out[column][row] += a[k][row] * b[column][k];
            }
        }
    }
    out
}

fn transform_point(m: [[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

fn transform_normal(m: [[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    let a = [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ];
    let cofactors = [
        [
            a[1][1] * a[2][2] - a[1][2] * a[2][1],
            a[1][2] * a[2][0] - a[1][0] * a[2][2],
            a[1][0] * a[2][1] - a[1][1] * a[2][0],
        ],
        [
            a[0][2] * a[2][1] - a[0][1] * a[2][2],
            a[0][0] * a[2][2] - a[0][2] * a[2][0],
            a[0][1] * a[2][0] - a[0][0] * a[2][1],
        ],
        [
            a[0][1] * a[1][2] - a[0][2] * a[1][1],
            a[0][2] * a[1][0] - a[0][0] * a[1][2],
            a[0][0] * a[1][1] - a[0][1] * a[1][0],
        ],
    ];
    let determinant =
        a[0][0] * cofactors[0][0] + a[0][1] * cofactors[0][1] + a[0][2] * cofactors[0][2];
    if determinant.abs() <= f32::EPSILON {
        return p;
    }
    [
        (cofactors[0][0] * p[0] + cofactors[0][1] * p[1] + cofactors[0][2] * p[2]) / determinant,
        (cofactors[1][0] * p[0] + cofactors[1][1] * p[1] + cofactors[1][2] * p[2]) / determinant,
        (cofactors[2][0] * p[0] + cofactors[2][1] * p[1] + cofactors[2][2] * p[2]) / determinant,
    ]
}

fn point3(v: [f32; 3]) -> [f32; 3] {
    v
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        [v[0] / length, v[1] / length, v[2] / length]
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn generate_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0; 3]; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let a = positions[tri[0] as usize];
        let b = positions[tri[1] as usize];
        let c = positions[tri[2] as usize];
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &index in tri {
            let normal = &mut normals[index as usize];
            for axis in 0..3 {
                normal[axis] += n[axis];
            }
        }
    }
    normals.into_iter().map(normalize).collect()
}

fn bound(positions: &[[f32; 3]]) -> Bound {
    let mut center = [0.0; 3];
    for p in positions {
        for axis in 0..3 {
            center[axis] += p[axis] / positions.len() as f32;
        }
    }
    let radius = positions
        .iter()
        .map(|p| (0..3).map(|i| (p[i] - center[i]).powi(2)).sum::<f32>())
        .fold(0.0_f32, f32::max)
        .sqrt();
    Bound {
        center: Vec3::new(center[0], center[1], center[2]),
        radius,
    }
}

#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend(v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend(v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend(v.to_le_bytes());
    }
    fn sized(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.0.extend(value.as_bytes());
    }
    fn refs(&mut self, values: &[i32]) {
        self.u32(values.len() as u32);
        for &value in values {
            self.i32(value);
        }
    }
    fn export_string(&mut self, value: &str) {
        self.u8(value.len() as u8 + 1);
        self.0.extend(value.as_bytes());
        self.u8(0);
    }
}

struct NifWriter {
    strings: Vec<String>,
    blocks: Vec<(String, Vec<u8>)>,
}

impl NifWriter {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            blocks: Vec::new(),
        }
    }
    fn string(&mut self, value: &str) -> u32 {
        if let Some(index) = self.strings.iter().position(|item| item == value) {
            return index as u32;
        }
        self.strings.push(value.to_owned());
        (self.strings.len() - 1) as u32
    }
    fn add(&mut self, name: &str, data: Bytes) -> i32 {
        self.blocks.push((name.to_owned(), data.0));
        (self.blocks.len() - 1) as i32
    }
    fn texture_set(&mut self, paths: &[String]) -> i32 {
        let mut data = Bytes::default();
        data.u32(paths.len() as u32);
        for path in paths {
            data.sized(path);
        }
        self.add("BSShaderTextureSet", data)
    }
    fn shader(&mut self, texture: i32, material: &Material) -> i32 {
        let mut data = Bytes::default();
        data.u32(match material.kind {
            crate::mesh::MaterialKind::Lighting(kind) => match kind {
                crate::blocks::ShaderType::LodLandscapeNoise => 18,
                crate::blocks::ShaderType::LodLandscape => 9,
                _ => 0,
            },
            _ => 0,
        });
        data.u32(u32::MAX);
        data.refs(&[]);
        data.i32(-1);
        data.u32(material.flags1);
        data.u32(material.flags2);
        data.f32(material.uv_offset[0]);
        data.f32(material.uv_offset[1]);
        data.f32(material.uv_scale[0]);
        data.f32(material.uv_scale[1]);
        data.i32(texture);
        for value in material.emissive_color {
            data.f32(value);
        }
        data.f32(material.emissive_multiple);
        data.u32(3);
        data.f32(1.0);
        data.f32(0.0);
        data.f32(material.glossiness);
        for value in material.specular_color {
            data.f32(value);
        }
        data.f32(material.specular_strength);
        data.f32(0.3);
        data.f32(2.0);
        self.add("BSLightingShaderProperty", data)
    }
    fn shape(&mut self, mesh: &Mesh, shader: i32) -> i32 {
        let name = self.string(&mesh.name);
        let generated_normals;
        let normals = if mesh.normals.is_empty() {
            generated_normals = generate_normals(&mesh.positions, &mesh.indices);
            &generated_normals
        } else {
            &mesh.normals
        };
        let attrs = attr::VERTEX | attr::NORMALS | if mesh.uvs.is_empty() { 0 } else { attr::UVS };
        let desc = VertexDesc::from_attributes(attrs, BS_SE);
        let mut data = Bytes::default();
        data.u32(name);
        data.refs(&[]);
        data.i32(-1);
        data.u32(0xE);
        data.f32(0.0);
        data.f32(0.0);
        data.f32(0.0);
        for value in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            data.f32(value);
        }
        data.f32(1.0);
        data.i32(-1);
        let b = bound(&mesh.positions);
        data.f32(b.center.x);
        data.f32(b.center.y);
        data.f32(b.center.z);
        data.f32(b.radius);
        data.i32(-1);
        data.i32(shader);
        data.i32(-1);
        data.u64(desc.0);
        data.u16((mesh.indices.len() / 3) as u16);
        data.u16(mesh.positions.len() as u16);
        data.u32((desc.size_bytes() * mesh.positions.len() + mesh.indices.len() * 2) as u32);
        for (index, position) in mesh.positions.iter().enumerate() {
            for value in position {
                data.f32(*value);
            }
            data.u32(0);
            if !mesh.uvs.is_empty() {
                data.u16(to_half(mesh.uvs[index][0]));
                data.u16(to_half(mesh.uvs[index][1]));
            }
            for value in normals[index] {
                data.u8(to_norm(value));
            }
            data.u8(0);
        }
        for tri in mesh.indices.as_chunks::<3>().0 {
            for &index in tri {
                data.u16(index as u16);
            }
        }
        data.u32(0);
        self.add("BSTriShape", data)
    }
    fn node(&mut self, name: &str, children: &[i32]) -> i32 {
        let name = self.string(name);
        let mut data = Bytes::default();
        data.u32(name);
        data.refs(&[]);
        data.i32(-1);
        data.u32(0xE);
        data.f32(0.0);
        data.f32(0.0);
        data.f32(0.0);
        for value in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            data.f32(value);
        }
        data.f32(1.0);
        data.i32(-1);
        data.refs(children);
        data.u32(0);
        self.add("NiNode", data)
    }
    fn finish(self, root: i32) -> Result<Vec<u8>> {
        if self.blocks.len() > u16::MAX as usize {
            return Err(invalid("NIF block type table is too large"));
        }
        let mut output = Bytes::default();
        output.0.extend(b"Gamebryo File Format, Version 20.2.0.7\n");
        output.u32(NIF_VERSION);
        output.u8(1);
        output.u32(12);
        output.u32(self.blocks.len() as u32);
        output.u32(BS_SE);
        output.export_string("skyrim-open-engine");
        output.export_string("");
        output.export_string("");
        let mut types = Vec::<String>::new();
        for (name, _) in &self.blocks {
            if !types.contains(name) {
                types.push(name.clone());
            }
        }
        output.u16(types.len() as u16);
        for name in &types {
            output.sized(name);
        }
        for (name, _) in &self.blocks {
            output.u16(
                types
                    .iter()
                    .position(|kind| kind == name)
                    .ok_or_else(|| invalid("missing NIF block type"))? as u16,
            );
        }
        for (_, data) in &self.blocks {
            output.u32(data.len() as u32);
        }
        output.u32(self.strings.len() as u32);
        output.u32(self.strings.iter().map(String::len).max().unwrap_or(0) as u32);
        for string in &self.strings {
            output.sized(string);
        }
        output.u32(0);
        for (_, block) in &self.blocks {
            output.0.extend(block);
        }
        output.refs(&[root]);
        Ok(output.0)
    }
}

fn to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7fffff;
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let m = (mantissa | 0x800000) >> (1 - exponent);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exponent >= 31 {
        return sign | 0x7c00 | if mantissa == 0 { 0 } else { 0x0200 };
    }
    sign | ((exponent as u16) << 10) | ((mantissa + 0x1000) >> 13) as u16
}

fn to_norm(value: f32) -> u8 {
    ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0).round() as u8
}
