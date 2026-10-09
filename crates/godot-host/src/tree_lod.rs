// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim LST atlas types and BTT references from xEdit Core/wbLOD.pas,
//! TwbLodTES5TreeType, TwbLodTES5TreeRef and their LoadFromData writers.
//! The host builds crossed, alpha-tested quads from these existing assets.

use std::collections::BTreeMap;

use crate::mesh::{AlphaMode, MaterialDesc, MeshData};

/// Invalid archived tree LOD, rejected before geometry is built.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid tree LOD {format} at byte {offset}: {reason}")]
pub struct TreeLodError {
    pub format: &'static str,
    pub offset: usize,
    pub reason: &'static str,
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    format: &'static str,
}

impl Reader<'_> {
    fn error(&self, reason: &'static str) -> TreeLodError {
        TreeLodError {
            format: self.format,
            offset: self.offset,
            reason,
        }
    }

    fn word(&mut self) -> Result<u32, TreeLodError> {
        let bytes = self
            .bytes
            .get(self.offset..self.offset + 4)
            .ok_or_else(|| self.error("truncated field"))?;
        self.offset += 4;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn float(&mut self) -> Result<f32, TreeLodError> {
        let value = f32::from_bits(self.word()?);
        if !value.is_finite() {
            return Err(self.error("non-finite value"));
        }
        Ok(value)
    }

    fn count(&mut self, stride: usize) -> Result<usize, TreeLodError> {
        let count = self.word()? as usize;
        if count > (self.bytes.len() - self.offset) / stride {
            return Err(self.error("count exceeds remaining bytes"));
        }
        Ok(count)
    }
}

/// One atlas entry keyed by its stored index, which need not equal its row.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeType {
    pub width: f32,
    pub height: f32,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// One BTT placement in world-space Skyrim units. Reference IDs are preserved
/// as archived metadata, never treated as resolved load-order identities.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeReference {
    pub tree_type: u32,
    pub position: [f32; 3],
    pub rotation: f32,
    pub scale: f32,
    pub raw_form_id: u32,
}

/// Read the count and packed 32-byte LST entries, ignoring bytes after the table.
pub fn read_types(bytes: &[u8]) -> Result<BTreeMap<u32, TreeType>, TreeLodError> {
    let mut reader = Reader {
        bytes,
        offset: 0,
        format: "LST",
    };
    let count = reader.count(32)?;
    let mut types = BTreeMap::new();
    for _ in 0..count {
        let index = reader.word()?;
        let entry = TreeType {
            width: reader.float()?,
            height: reader.float()?,
            uv_min: [reader.float()?, reader.float()?],
            uv_max: [reader.float()?, reader.float()?],
        };
        reader.word()?;
        if entry.width <= 0.0 || entry.height <= 0.0 {
            return Err(reader.error("non-positive billboard dimensions"));
        }
        if entry.uv_min[0] == entry.uv_max[0] || entry.uv_min[1] == entry.uv_max[1] {
            return Err(reader.error("degenerate atlas rectangle"));
        }
        if types.insert(index, entry).is_some() {
            return Err(reader.error("duplicate type index"));
        }
    }
    Ok(types)
}

/// Read BTT type groups and their packed 32-byte reference arrays.
/// Unused trailing data is accepted, matching xEdit's LoadFromData reader
/// and the payloads shipped in several Solstheim blocks.
pub fn read_references(bytes: &[u8]) -> Result<Vec<TreeReference>, TreeLodError> {
    let mut reader = Reader {
        bytes,
        offset: 0,
        format: "BTT",
    };
    let count = reader.count(8)?;
    let mut references = Vec::new();
    for _ in 0..count {
        let tree_type = reader.word()?;
        let count = reader.count(32)?;
        for _ in 0..count {
            let entry = TreeReference {
                tree_type,
                position: [reader.float()?, reader.float()?, reader.float()?],
                rotation: reader.float()?,
                scale: reader.float()?,
                raw_form_id: reader.word()?,
            };
            reader.word()?;
            reader.word()?;
            if entry.scale <= 0.0 {
                return Err(reader.error("non-positive scale"));
            }
            references.push(entry);
        }
    }
    Ok(references)
}

/// Build two perpendicular atlas quads per reference, retaining authored
/// rotation, dimensions, scale and UVs. BTT positions already use world units.
pub fn build_billboards(
    types: &BTreeMap<u32, TreeType>,
    references: &[TreeReference],
    atlas: &str,
) -> Result<MeshData, TreeLodError> {
    let mut mesh = MeshData {
        uvs: Some(Vec::new()),
        material: MaterialDesc {
            diffuse: Some(atlas.into()),
            alpha: AlphaMode::Cutout(128),
            double_sided: true,
            ..Default::default()
        },
        ..Default::default()
    };
    for reference in references {
        let tree = types.get(&reference.tree_type).ok_or(TreeLodError {
            format: "BTT",
            offset: 0,
            reason: "reference names absent LST type",
        })?;
        for angle in [
            reference.rotation,
            reference.rotation + std::f32::consts::FRAC_PI_2,
        ] {
            let (sin, cos) = angle.sin_cos();
            let half = tree.width * reference.scale * 0.5;
            let height = tree.height * reference.scale;
            let [x, y, z] = reference.position;
            let base = u32::try_from(mesh.positions.len()).map_err(|_| TreeLodError {
                format: "BTT",
                offset: 0,
                reason: "billboard vertex count exceeds u32",
            })?;
            mesh.positions.extend([
                [x - cos * half, y - sin * half, z],
                [x + cos * half, y + sin * half, z],
                [x + cos * half, y + sin * half, z + height],
                [x - cos * half, y - sin * half, z + height],
            ]);
            if mesh
                .positions
                .iter()
                .rev()
                .take(4)
                .flatten()
                .any(|v| !v.is_finite())
            {
                return Err(TreeLodError {
                    format: "BTT",
                    offset: 0,
                    reason: "billboard geometry overflow",
                });
            }
            if let Some(uvs) = &mut mesh.uvs {
                let [u0, v0] = tree.uv_min;
                let [u1, v1] = tree.uv_max;
                uvs.extend([[u0, v1], [u1, v1], [u1, v0], [u0, v0]]);
            }
            mesh.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Ok(mesh)
}
