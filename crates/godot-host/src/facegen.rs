// SPDX-License-Identifier: GPL-3.0-or-later

//! Head-part morph application, tint composition and generated FaceGen assets.
//!
//! The morph path converts named TRI weights into host surface arrays. The tint
//! path composes decoded masks into a BC1 sRGB DDS and provides its game asset path.
//! Record layouts follow xEdit's NPC_ and HDPT definitions. Signed slider order
//! follows SKSE's FaceMorphList, with absolute weights as in RaceMenu's
//! FaceMorphInterface::ApplyMorph. TiltedEvolution resolves head parts before
//! looking up geometry. TRI decoding is shared with soe-nif's BodySlide reader.

use std::collections::HashSet;
use std::path::{Component, PathBuf};

use soe_dds::{dxgi, encode, Image};
use soe_esm::cursor::Fields;
use soe_esm::records::npc::{NpcFace, FACE_MORPH_COUNT};
use soe_esm::{GlobalFormId, LoadOrder, Record, Tag};
use soe_nif::{HeadTri, Mesh as NifMesh};
use thiserror::Error;

use crate::data_source::mesh_data_from;
use crate::mesh::{build_surface, MeshError, MeshOptions, SurfaceArrays};

/// Errors while applying head morphs and preparing a renderable surface.
#[derive(Debug, thiserror::Error)]
pub enum FaceGenError {
    #[error(transparent)]
    Nif(#[from] soe_nif::Error),
    #[error(transparent)]
    Mesh(#[from] MeshError),
    #[error(transparent)]
    Record(#[from] soe_esm::Error),
    #[error(transparent)]
    Asset(#[from] soe_bsa::Error),
    #[error("FaceGen form {form}: {reason}")]
    InvalidRecord { form: GlobalFormId, reason: String },
}

const NAM9_NAMES: [(&str, &str); FACE_MORPH_COUNT] = [
    ("NoseShort", "NoseLong"),
    ("NoseDown", "NoseUp"),
    ("JawUp", "JawDown"),
    ("JawNarrow", "JawWide"),
    ("JawBack", "JawForward"),
    ("CheeksDown", "CheeksUp"),
    ("CheeksIn", "CheeksOut"),
    ("EyesMoveDown", "EyesMoveUp"),
    ("EyesMoveIn", "EyesMoveOut"),
    ("BrowDown", "BrowUp"),
    ("BrowIn", "BrowOut"),
    ("BrowBack", "BrowForward"),
    ("LipMoveDown", "LipMoveUp"),
    ("LipMoveIn", "LipMoveOut"),
    ("ChinThin", "ChinWide"),
    ("ChinMoveUp", "ChinMoveDown"),
    ("Overbite", "Underbite"),
    ("EyesBack", "EyesForward"),
    ("VampireMorph", "VampireMorph"),
];

/// Map the signed NPC NAM9 sliders to named TRI targets in record field order.
/// Zero and the FaceData default sentinel do not select a target. Negative
/// values select the lower target and positive values select the upper target.
///
/// # Errors
/// Returns a record error for a non-finite slider value.
pub fn nam9_morph_weights(
    values: &[f32; FACE_MORPH_COUNT],
) -> Result<Vec<(&'static str, f32)>, FaceGenError> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(soe_esm::Error::Invalid("NPC face morph values must be finite".into()).into());
    }
    Ok(values
        .iter()
        .zip(NAM9_NAMES)
        .filter(|(value, _)| **value != 0.0 && **value != f32::MAX)
        .map(|(value, (lower, upper))| (if *value < 0.0 { lower } else { upper }, value.abs()))
        .collect())
}

/// One generated shape, retaining its HDPT identity and renderer-neutral mesh.
#[derive(Debug)]
pub struct GeneratedHeadPart {
    /// Resolved winning HDPT form that declared this shape's assets.
    pub form: GlobalFormId,
    /// Generated geometry in Skyrim coordinates, with authored mesh metadata.
    pub mesh: NifMesh,
    /// The same geometry converted to Godot surface arrays.
    pub surface: SurfaceArrays,
}

fn invalid_record(form: GlobalFormId, reason: impl Into<String>) -> FaceGenError {
    FaceGenError::InvalidRecord {
        form,
        reason: reason.into(),
    }
}

fn asset_path(value: &str) -> String {
    let path = value.replace('\\', "/");
    if path.to_ascii_lowercase().starts_with("meshes/") {
        path
    } else {
        format!("meshes/{path}")
    }
}

fn chargen_path(record: &Record, form: GlobalFormId) -> Result<Option<String>, FaceGenError> {
    let mut kind = None;
    let mut path = None;
    for sub in &record.subrecords {
        let mut fields = Fields::new(record.tag, sub.tag, &sub.data);
        if sub.tag == Tag(*b"NAM0") {
            if kind.is_some() {
                return Err(invalid_record(form, "HDPT NAM0 has no NAM1 filename"));
            }
            fields.exact(4)?;
            kind = Some(fields.u32()?);
        } else if sub.tag == Tag(*b"NAM1") {
            let part_kind = kind
                .take()
                .ok_or_else(|| invalid_record(form, "orphan HDPT NAM1"))?;
            if part_kind == 2 {
                if path.is_some() {
                    return Err(invalid_record(form, "repeated chargen TRI declaration"));
                }
                path = Some(soe_esm::cursor::zstring(&sub.data));
            }
        }
    }
    if kind.is_some() {
        return Err(invalid_record(form, "HDPT NAM0 has no NAM1 filename"));
    }
    Ok(path.filter(|path| !path.is_empty()))
}

/// Generate a winning NPC's declared PNAM head parts and their HNAM extras.
///
/// Links resolve in each winning record's source namespace. Assets use the
/// supplied load-order VFS. NAM0 type 2 selects the chargen TRI, independently
/// of the expression TRI. Only the unique shape matching that TRI's base
/// vertices is morphed. Targets absent from a part's TRI leave that part alone.
/// Shapes without a chargen TRI retain their authored geometry. Results follow
/// PNAM order, then depth-first HNAM order, with shared extras emitted once.
///
/// # Errors
/// Returns typed errors for absent, deleted or wrongly typed forms, malformed
/// records or assets, cyclic extras, or ambiguous or mismatched TRI geometry.
pub fn generate_npc_head(
    order: &LoadOrder,
    assets: &soe_bsa::Vfs,
    npc: GlobalFormId,
) -> Result<Vec<GeneratedHeadPart>, FaceGenError> {
    let source = order
        .winner_ref(npc)
        .ok_or_else(|| invalid_record(npc, "missing NPC"))?;
    let record = order.record(source);
    if record.flags & soe_esm::plugin::flags::DELETED != 0 {
        return Err(invalid_record(npc, "deleted NPC"));
    }
    let face = NpcFace::from_record(record)?;
    let weights = nam9_morph_weights(&face.morph)?;
    let mut pending: Vec<_> = face
        .head_parts
        .iter()
        .rev()
        .map(|raw| (order.resolve(source.file, *raw), false))
        .collect();
    let mut visiting = HashSet::new();
    let mut complete = HashSet::new();
    let mut result = Vec::new();
    while let Some((form, leaving)) = pending.pop() {
        if leaving {
            visiting.remove(&form);
            complete.insert(form);
            continue;
        }
        if complete.contains(&form) {
            continue;
        }
        if !visiting.insert(form) {
            return Err(invalid_record(form, "cyclic HDPT extra parts"));
        }
        let source = order
            .winner_ref(form)
            .ok_or_else(|| invalid_record(form, "missing head part"))?;
        let record = order.record(source);
        if record.tag != Tag(*b"HDPT") || record.flags & soe_esm::plugin::flags::DELETED != 0 {
            return Err(invalid_record(form, "head part must be a live HDPT"));
        }
        let model = record
            .find(Tag(*b"MODL"))
            .ok_or_else(|| invalid_record(form, "head part has no MODL"))?;
        let model = soe_esm::cursor::zstring(model);
        if model.is_empty() {
            return Err(invalid_record(form, "empty head-part MODL"));
        }
        let meshes = soe_nif::parse(&assets.read(&asset_path(&model))?)?.meshes()?;
        if meshes.is_empty() {
            return Err(invalid_record(form, "head-part NIF has no shapes"));
        }
        let tri = chargen_path(record, form)?
            .map(|path| {
                Ok::<_, FaceGenError>(soe_nif::parse_head_tri(&assets.read(&asset_path(&path))?)?)
            })
            .transpose()?;
        let matched = if let Some(tri) = &tri {
            let matching: Vec<_> = meshes
                .iter()
                .enumerate()
                .filter(|(_, mesh)| {
                    mesh.positions.len() == tri.vertices().len()
                        && mesh
                            .positions
                            .iter()
                            .zip(tri.vertices())
                            .all(|(a, b)| a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1.0e-4))
                })
                .map(|(index, _)| index)
                .collect();
            if matching.len() != 1 {
                return Err(invalid_record(
                    form,
                    "chargen TRI must match exactly one NIF shape",
                ));
            }
            matching.first().copied()
        } else {
            None
        };
        for (index, mesh) in meshes.into_iter().enumerate() {
            let mesh = if let Some(tri) = tri.as_ref().filter(|_| matched == Some(index)) {
                let selected: Vec<_> = weights
                    .iter()
                    .copied()
                    .filter(|(name, _)| tri.morphs().iter().any(|morph| morph.name() == *name))
                    .collect();
                tri.apply_to_mesh(&mesh, &selected)?
            } else {
                mesh
            };
            let surface = build_surface(
                &mesh_data_from(&mesh, &mesh.transform),
                MeshOptions::default(),
            )?;
            result.push(GeneratedHeadPart {
                form,
                mesh,
                surface,
            });
        }
        pending.push((form, true));
        for data in record
            .all(Tag(*b"HNAM"))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let mut fields = Fields::new(record.tag, Tag(*b"HNAM"), data);
            fields.exact(4)?;
            pending.push((order.resolve(source.file, fields.u32()?), false));
        }
    }
    Ok(result)
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
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
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

/// Skyrim's FaceGen tint render is 512 pixels on each side.
pub const FACE_TINT_SIZE: u32 = 512;

/// One tint mask and the color and opacity applied to it.
#[derive(Debug, Clone, Copy)]
pub struct FaceTintLayer<'a> {
    /// Decoded mask pixels. The alpha channel controls where the tint applies.
    pub mask: &'a Image,
    /// The layer color in RGBA channel order, with alpha ignored.
    pub color: [u8; 4],
    /// Layer opacity from zero to one. Values outside the range are clamped.
    pub alpha: f32,
}

/// Failures while validating or writing a FaceGen tint texture.
#[derive(Debug, Error)]
pub enum FaceTintError {
    /// A layer mask does not match the FaceGen render dimensions.
    #[error("FaceGen tint mask must be {FACE_TINT_SIZE} by {FACE_TINT_SIZE} RGBA8")]
    InvalidMask,
    /// A plugin name is not a single file name component.
    #[error("plugin name must be a single file name")]
    InvalidPluginName,
    /// A local form id uses bits outside Skyrim's 24-bit record id.
    #[error("local form id must fit in 24 bits")]
    InvalidFormId,
    /// DDS encoding failed.
    #[error(transparent)]
    Dds(#[from] soe_dds::Error),
}

/// Build the relative game path for an NPC FaceGen tint texture.
///
/// `local_form_id` is the low 24-bit record id. The output uses the eight
/// digit uppercase spelling used by Skyrim asset tools.
///
/// # Errors
/// Returns [`FaceTintError::InvalidPluginName`] when the plugin name is not
/// one file name component.
pub fn face_tint_path(plugin_name: &str, local_form_id: u32) -> Result<PathBuf, FaceTintError> {
    let plugin_path = PathBuf::from(plugin_name);
    let mut components = plugin_path.components();
    if plugin_name.contains('\\')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(FaceTintError::InvalidPluginName);
    }
    if plugin_name.is_empty() {
        return Err(FaceTintError::InvalidPluginName);
    }
    if local_form_id > 0x00FF_FFFF {
        return Err(FaceTintError::InvalidFormId);
    }
    Ok(
        PathBuf::from("textures/actors/character/facegendata/facetint")
            .join(plugin_name)
            .join(format!("{local_form_id:08X}.dds")),
    )
}

/// Composite ordered tint masks and encode the result as a BC1 sRGB DDS.
///
/// Layers are applied in input order. Each mask's alpha is multiplied by the
/// layer opacity, then its color is blended over black using source-over
/// alpha. The final texture is opaque because FaceTint is a color mask.
///
/// # Errors
/// Returns [`FaceTintError::InvalidMask`] if any mask has another size or is
/// not a single RGBA8 surface. DDS encoding errors are returned unchanged.
pub fn generate_face_tint(layers: &[FaceTintLayer<'_>]) -> Result<Vec<u8>, FaceTintError> {
    let pixels = (FACE_TINT_SIZE * FACE_TINT_SIZE) as usize;
    let mut rgba = vec![0_u8; pixels * 4];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }

    for layer in layers {
        if layer.mask.width != FACE_TINT_SIZE
            || layer.mask.height != FACE_TINT_SIZE
            || layer.mask.depth != 1
            || layer.mask.rgba.len() != pixels * 4
        {
            return Err(FaceTintError::InvalidMask);
        }
        let opacity = layer.alpha.clamp(0.0, 1.0);
        let (dst_pixels, _) = rgba.as_chunks_mut::<4>();
        let (mask_pixels, _) = layer.mask.rgba.as_chunks::<4>();
        for (dst, mask) in dst_pixels.iter_mut().zip(mask_pixels) {
            let source_alpha = f32::from(mask[3]) / 255.0 * opacity;
            for (dst_channel, source_channel) in dst.iter_mut().take(3).zip(layer.color.iter()) {
                let source = f32::from(*source_channel);
                let destination = f32::from(*dst_channel);
                *dst_channel =
                    (source * source_alpha + destination * (1.0 - source_alpha)).round() as u8;
            }
        }
    }

    encode::encode_rgba8(
        FACE_TINT_SIZE,
        FACE_TINT_SIZE,
        &rgba,
        1,
        dxgi::BC1_UNORM_SRGB,
    )
    .map_err(FaceTintError::from)
}
