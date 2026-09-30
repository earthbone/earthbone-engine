// SPDX-License-Identifier: GPL-3.0-or-later

//! FaceGen head TRI morph data, following BodySlide's TriHeadFile reader and
//! the published NIF morph definitions in nif.xml.

use crate::{error::Error, stream::Stream, Result};

/// One named FaceGen head morph with one decoded delta per TRI vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadMorph {
    name: String,
    deltas: Vec<[f32; 3]>,
}

impl HeadMorph {
    /// The morph name stored in the TRI file.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Position deltas in TRI vertex order.
    #[must_use]
    pub fn deltas(&self) -> &[[f32; 3]] {
        &self.deltas
    }
}

/// Parsed FaceGen head TRI data.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadTri {
    vertices: Vec<[f32; 3]>,
    morphs: Vec<HeadMorph>,
}

impl HeadTri {
    /// Base vertices stored by the TRI file.
    #[must_use]
    pub fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }

    /// The morphs in file order.
    #[must_use]
    pub fn morphs(&self) -> &[HeadMorph] {
        &self.morphs
    }

    /// Apply one morph to a mesh's base positions without changing the input.
    ///
    /// A zero weight returns an exact copy of the base positions. The mesh and
    /// TRI vertex counts must match because deltas are indexed by vertex order.
    ///
    /// # Errors
    /// Returns an error for a missing morph, a non-finite weight, or a vertex
    /// count mismatch.
    pub fn apply_morph(
        &self,
        base_positions: &[[f32; 3]],
        morph_name: &str,
        weight: f32,
    ) -> Result<Vec<[f32; 3]>> {
        if !weight.is_finite() {
            return Err(Error::InvalidHeadTri("morph weight is not finite".into()));
        }
        if base_positions.len() != self.vertices.len() {
            return Err(Error::HeadMorphVertexCount {
                expected: self.vertices.len(),
                actual: base_positions.len(),
            });
        }
        let morph = self
            .morphs
            .iter()
            .find(|morph| morph.name == morph_name)
            .ok_or_else(|| Error::HeadMorphNotFound(morph_name.to_owned()))?;
        if weight == 0.0 {
            return Ok(base_positions.to_vec());
        }
        Ok(base_positions
            .iter()
            .zip(&morph.deltas)
            .map(|(base, delta)| {
                [
                    base[0] + delta[0] * weight,
                    base[1] + delta[1] * weight,
                    base[2] + delta[2] * weight,
                ]
            })
            .collect())
    }
}

/// Parse a FaceGen `FRTRI003` head morph file.
///
/// Morph vectors are signed 16-bit components multiplied by each morph's
/// stored scale. Mesh topology, UV coordinates, and modifier index lists are
/// consumed to locate the morph section but are not retained.
///
/// # Errors
/// Returns a typed error for truncated data, invalid counts, unsupported
/// identifiers or versions, invalid numeric values, and malformed topology.
pub fn parse_head_tri(data: &[u8]) -> Result<HeadTri> {
    let mut stream = Stream::new(data);
    if stream.take(2)? != b"FR" || stream.take(3)? != b"TRI" {
        return Err(Error::InvalidHeadTri("expected FRTRI identifier".into()));
    }
    if stream.take(3)? != b"003" {
        return Err(Error::InvalidHeadTri("unsupported TRI version".into()));
    }

    let num_vertices = stream.u32()?;
    let num_triangles = stream.u32()?;
    stream.u32()?;
    for _ in 0..2 {
        stream.u32()?;
    }
    let num_uv = stream.u32()?;
    stream.u32()?;
    let num_morphs = stream.u32()?;
    let num_modifiers = stream.u32()?;
    let num_modifier_vertices = stream.u32()?;
    for _ in 0..4 {
        stream.u32()?;
    }

    let vertex_count = stream.count(num_vertices, 12)?;
    let vertices = (0..vertex_count)
        .map(|_| {
            let vertex = stream.vec3()?.to_array();
            if vertex.iter().any(|component| !component.is_finite()) {
                return Err(Error::InvalidHeadTri("base vertex is not finite".into()));
            }
            Ok(vertex)
        })
        .collect::<Result<Vec<_>>>()?;

    let modifier_vertex_count = stream.count(num_modifier_vertices, 12)?;
    stream.take(modifier_vertex_count * 12)?;

    for _ in 0..stream.count(num_triangles, 12)? {
        for _ in 0..3 {
            let index = stream.u32()?;
            if index >= num_vertices {
                return Err(Error::InvalidHeadTri(format!(
                    "triangle index {index} exceeds vertex count {num_vertices}"
                )));
            }
        }
    }
    stream.take(stream.count(num_uv, 8)? * 8)?;
    for _ in 0..num_triangles {
        for _ in 0..3 {
            stream.u32()?;
        }
    }

    let mut morphs = Vec::with_capacity(stream.count(num_morphs, 8)?);
    for _ in 0..num_morphs {
        let name = stream.sized_string()?;
        let multiplier = stream.f32()?;
        if !multiplier.is_finite() {
            return Err(Error::InvalidHeadTri("morph scale is not finite".into()));
        }
        let mut deltas = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            let delta = [
                f32::from(stream.u16()? as i16) * multiplier,
                f32::from(stream.u16()? as i16) * multiplier,
                f32::from(stream.u16()? as i16) * multiplier,
            ];
            if delta.iter().any(|component| !component.is_finite()) {
                return Err(Error::InvalidHeadTri("morph delta is not finite".into()));
            }
            deltas.push(delta);
        }
        morphs.push(HeadMorph { name, deltas });
    }

    let modifier_count = stream.count(num_modifiers, 8)?;
    for _ in 0..modifier_count {
        let _name = stream.sized_string()?;
        let index_count = stream.u32()?;
        let index_count = stream.count(index_count, 4)?;
        stream.take(index_count * 4)?;
    }

    Ok(HeadTri { vertices, morphs })
}
