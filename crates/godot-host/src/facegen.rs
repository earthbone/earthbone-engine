// SPDX-License-Identifier: GPL-3.0-or-later

//! Head-part morph application, tint composition and generated FaceGen assets.
//!
//! The morph path converts named TRI weights into host surface arrays. The tint
//! path composes decoded masks into a BC1 sRGB DDS and provides its game asset path.

use std::path::{Component, PathBuf};

use soe_dds::{dxgi, encode, Image};
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
