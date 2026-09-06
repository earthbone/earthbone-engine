// SPDX-License-Identifier: GPL-3.0-or-later

//! The packed vertex format of `BSTriShape` and the SSE `NiSkinPartition`
//! global vertex buffer. `BSVertexDesc` is a 64-bit word: the low nibble is
//! the vertex size in 4-byte units, the next nibbles are attribute offsets
//! the reader does not need (it reads fields in order), and bits 44 and up
//! say which attributes are present.

use crate::error::Result;
use crate::header::{Header, BS_FALLOUT_4};
use crate::stream::Stream;
use crate::types::Vec3;

pub mod attr {
    pub const VERTEX: u16 = 1 << 0;
    pub const UVS: u16 = 1 << 1;
    pub const UVS_2: u16 = 1 << 2;
    pub const NORMALS: u16 = 1 << 3;
    pub const TANGENTS: u16 = 1 << 4;
    pub const COLORS: u16 = 1 << 5;
    pub const SKINNED: u16 = 1 << 6;
    pub const LAND_DATA: u16 = 1 << 7;
    pub const EYE_DATA: u16 = 1 << 8;
    pub const INSTANCE: u16 = 1 << 9;
    /// Fallout 4 only: positions are floats rather than halves.
    pub const FULL_PRECISION: u16 = 1 << 10;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VertexDesc(pub u64);

impl VertexDesc {
    pub fn size_bytes(self) -> usize {
        ((self.0 & 0xF) * 4) as usize
    }

    pub fn attributes(self) -> u16 {
        ((self.0 >> 44) & 0xFFF) as u16
    }

    pub fn has(self, flag: u16) -> bool {
        self.attributes() & flag == flag
    }

    /// Build a descriptor from attribute flags, computing the size the way
    /// the game does. Used by writers and by the tests.
    pub fn from_attributes(attrs: u16, bs_version: u32) -> VertexDesc {
        let size = vertex_size(attrs, bs_version);
        VertexDesc(((attrs as u64) << 44) | ((size / 4) as u64 & 0xF))
    }
}

/// Size in bytes of one packed vertex with these attributes.
pub fn vertex_size(attrs: u16, bs_version: u32) -> usize {
    let mut n = 0;
    if attrs & attr::VERTEX != 0 {
        let full = bs_version < BS_FALLOUT_4 || attrs & attr::FULL_PRECISION != 0;
        n += if full { 16 } else { 8 };
    }
    if attrs & attr::UVS != 0 {
        n += 4;
    }
    if attrs & attr::NORMALS != 0 {
        n += 4;
    }
    if attrs & (attr::NORMALS | attr::TANGENTS) == (attr::NORMALS | attr::TANGENTS) {
        n += 4;
    }
    if attrs & attr::COLORS != 0 {
        n += 4;
    }
    if attrs & attr::SKINNED != 0 {
        n += 12;
    }
    if attrs & attr::EYE_DATA != 0 {
        n += 4;
    }
    n
}

/// One unpacked vertex. Which fields carry data is decided by the
/// descriptor the vertex was read with, absent fields are zero.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vertex {
    pub position: Vec3,
    pub uv: [f32; 2],
    pub normal: Vec3,
    pub tangent: Vec3,
    pub bitangent: Vec3,
    pub color: [f32; 4],
    pub bone_weights: [f32; 4],
    pub bone_indices: [u8; 4],
    pub eye_data: f32,
}

fn norm_byte(b: u8) -> f32 {
    (b as f32) / 255.0 * 2.0 - 1.0
}

pub fn read_vertex(s: &mut Stream<'_>, attrs: u16, h: &Header) -> Result<Vertex> {
    let mut v = Vertex::default();
    if attrs & attr::VERTEX != 0 {
        let full = h.bs_version < BS_FALLOUT_4 || attrs & attr::FULL_PRECISION != 0;
        if full {
            v.position = s.vec3()?;
            if attrs & attr::TANGENTS != 0 {
                v.bitangent.x = s.f32()?;
            } else {
                s.u32()?;
            }
        } else {
            v.position = Vec3::new(s.half()?, s.half()?, s.half()?);
            if attrs & attr::TANGENTS != 0 {
                v.bitangent.x = s.half()?;
            } else {
                s.u16()?;
            }
        }
    }
    if attrs & attr::UVS != 0 {
        v.uv = [s.half()?, s.half()?];
    }
    if attrs & attr::NORMALS != 0 {
        v.normal = Vec3::new(norm_byte(s.u8()?), norm_byte(s.u8()?), norm_byte(s.u8()?));
        v.bitangent.y = norm_byte(s.u8()?);
        if attrs & attr::TANGENTS != 0 {
            v.tangent = Vec3::new(norm_byte(s.u8()?), norm_byte(s.u8()?), norm_byte(s.u8()?));
            v.bitangent.z = norm_byte(s.u8()?);
        }
    }
    if attrs & attr::COLORS != 0 {
        let b = s.take(4)?;
        v.color = [
            b[0] as f32 / 255.0,
            b[1] as f32 / 255.0,
            b[2] as f32 / 255.0,
            b[3] as f32 / 255.0,
        ];
    }
    if attrs & attr::SKINNED != 0 {
        v.bone_weights = [s.half()?, s.half()?, s.half()?, s.half()?];
        let b = s.take(4)?;
        v.bone_indices = [b[0], b[1], b[2], b[3]];
    }
    if attrs & attr::EYE_DATA != 0 {
        v.eye_data = s.f32()?;
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_packs_size_and_attributes() {
        let attrs = attr::VERTEX | attr::UVS | attr::NORMALS | attr::TANGENTS;
        let d = VertexDesc::from_attributes(attrs, 100);
        assert_eq!(d.attributes(), attrs);
        assert_eq!(d.size_bytes(), 28);
        assert!(d.has(attr::TANGENTS));
        assert!(!d.has(attr::SKINNED));
        let skinned = VertexDesc::from_attributes(attrs | attr::SKINNED | attr::COLORS, 100);
        assert_eq!(skinned.size_bytes(), 44);
    }

    #[test]
    fn fallout4_positions_are_half_unless_full_precision() {
        assert_eq!(vertex_size(attr::VERTEX, 130), 8);
        assert_eq!(vertex_size(attr::VERTEX | attr::FULL_PRECISION, 130), 16);
        assert_eq!(vertex_size(attr::VERTEX, 100), 16);
    }
}
