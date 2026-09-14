// SPDX-License-Identifier: GPL-3.0-or-later

//! STAT: a static object, the record most references in a cell point at.

use crate::cursor::{zstring, Fields};
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::zstring_field;

pub mod flags {
    pub const NEVER_FADES: u32 = 1 << 2;
    pub const HAS_TREE_LOD: u32 = 1 << 6;
    pub const ADD_ON_LOD_OBJECT: u32 = 1 << 7;
    pub const HIDDEN_FROM_LOCAL_MAP: u32 = 1 << 9;
    pub const HAS_DISTANT_LOD: u32 = 1 << 15;
    pub const USES_HD_LOD_TEXTURE: u32 = 1 << 17;
    pub const HAS_CURRENTS: u32 = 1 << 19;
    pub const IS_MARKER: u32 = 1 << 23;
    pub const OBSTACLE: u32 = 1 << 25;
    pub const NAVMESH_FILTER: u32 = 1 << 26;
    pub const NAVMESH_BOUNDING_BOX: u32 = 1 << 27;
    pub const SHOW_IN_WORLD_MAP: u32 = 1 << 28;
    pub const NAVMESH_GROUND: u32 = 1 << 30;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionMaterial {
    /// Degrees, 30 to 120.
    pub max_angle: f32,
    pub material: u32,
    pub considered_snow: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Static {
    pub form_id: u32,
    pub record_flags: u32,
    pub editor_id: Option<String>,
    /// OBND: min x y z, max x y z, in world units.
    pub bounds: Option<[i16; 6]>,
    /// MODL: the NIF path relative to `Data/Meshes`.
    pub model: Option<String>,
    pub direction_material: Option<DirectionMaterial>,
    /// MNAM: four LOD model paths, level 0 to 3, empty when unused.
    pub lod_models: Option<[String; 4]>,
}

impl Static {
    pub fn from_record(rec: &Record) -> Result<Static> {
        debug_assert_eq!(rec.tag, tag::STAT);
        let bounds = match rec.find(Tag::new(b"OBND")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"OBND"), d);
                f.exact(12)?;
                let mut b = [0i16; 6];
                for v in b.iter_mut() {
                    *v = f.i16()?;
                }
                Some(b)
            }
        };
        let direction_material = match rec.find(Tag::new(b"DNAM")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"DNAM"), d);
                if d.len() != 8 && d.len() != 12 {
                    f.exact(12)?;
                }
                let max_angle = f.f32()?;
                let material = f.u32()?;
                let considered_snow = f.remaining() >= 1 && f.u8()? != 0;
                Some(DirectionMaterial {
                    max_angle,
                    material,
                    considered_snow,
                })
            }
        };
        let lod_models = match rec.find(Tag::new(b"MNAM")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"MNAM"), d);
                f.exact(4 * 260)?;
                let mut out: [String; 4] = Default::default();
                for s in out.iter_mut() {
                    *s = zstring(f.bytes(260)?);
                }
                Some(out)
            }
        };
        Ok(Static {
            form_id: rec.form_id,
            record_flags: rec.flags,
            editor_id: rec.editor_id(),
            bounds,
            model: zstring_field(rec, Tag::new(b"MODL")),
            direction_material,
            lod_models,
        })
    }

    pub fn has_distant_lod(&self) -> bool {
        self.record_flags & flags::HAS_DISTANT_LOD != 0
    }

    pub fn is_marker(&self) -> bool {
        self.record_flags & flags::IS_MARKER != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_static_reads_its_model_bounds_and_lod() {
        let mut obnd = Vec::new();
        for v in [-10i16, -20, -30, 10, 20, 30] {
            obnd.extend_from_slice(&v.to_le_bytes());
        }
        let mut mnam = vec![0u8; 1040];
        mnam[..9].copy_from_slice(b"lod0.nif\0");
        mnam[260..269].copy_from_slice(b"lod1.nif\0");
        let mut dnam = 90.0f32.to_le_bytes().to_vec();
        dnam.extend_from_slice(&0x55u32.to_le_bytes());
        dnam.extend_from_slice(&[1, 0, 0, 0]);
        let rec = Record::new(tag::STAT, 0x800)
            .with_flags(flags::HAS_DISTANT_LOD)
            .zsub(tag::EDID, "WRTowerRoof")
            .sub(b"OBND", obnd)
            .zsub(b"MODL", "Architecture\\Whiterun\\WRTowerRoof.nif")
            .sub(b"DNAM", dnam)
            .sub(b"MNAM", mnam);
        let s = Static::from_record(&rec).unwrap();
        assert_eq!(s.bounds, Some([-10, -20, -30, 10, 20, 30]));
        assert_eq!(
            s.model.as_deref(),
            Some("Architecture\\Whiterun\\WRTowerRoof.nif")
        );
        assert!(s.has_distant_lod());
        assert!(s.direction_material.unwrap().considered_snow);
        let lod = s.lod_models.unwrap();
        assert_eq!(lod[0], "lod0.nif");
        assert_eq!(lod[1], "lod1.nif");
        assert_eq!(lod[3], "");
    }

    #[test]
    fn a_bad_bounds_block_is_an_error() {
        let rec = Record::new(tag::STAT, 1).sub(b"OBND", [0u8; 11]);
        assert!(Static::from_record(&rec).is_err());
    }
}
