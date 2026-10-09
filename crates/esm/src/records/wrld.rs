// SPDX-License-Identifier: GPL-3.0-or-later

//! WRLD: a worldspace, the container for exterior cells.

use crate::cursor::Fields;
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::{f32_field, form_id, text, zstring_field, Text};

pub mod flags {
    pub const SMALL_WORLD: u8 = 0x01;
    pub const CANT_FAST_TRAVEL: u8 = 0x02;
    pub const NO_LOD_WATER: u8 = 0x08;
    pub const NO_LANDSCAPE: u8 = 0x10;
    pub const NO_SKY: u8 = 0x20;
    pub const FIXED_DIMENSIONS: u8 = 0x40;
    pub const NO_GRASS: u8 = 0x80;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapData {
    pub usable_width: u32,
    pub usable_height: u32,
    pub north_west: (i16, i16),
    pub south_east: (i16, i16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub form_id: u32,
    /// Runtime north rotation. WRLD plugin records do not serialize this value.
    pub north_rotation: f32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub flags: u8,
    /// WNAM: the worldspace whose data this one inherits per PNAM.
    pub parent: Option<u32>,
    pub parent_flags: Option<u16>,
    pub climate: Option<u32>,
    pub water: Option<u32>,
    pub lod_water: Option<u32>,
    pub lod_water_height: Option<f32>,
    /// WCTR: the cell the world's map and loading are centred on.
    pub center: Option<(i16, i16)>,
    /// NAM0 and NAM9: the used extent in cell units as floats.
    pub bounds_min: Option<(f32, f32)>,
    pub bounds_max: Option<(f32, f32)>,
    pub map: Option<MapData>,
    pub distant_lod_multiplier: Option<f32>,
    pub default_land_height: Option<f32>,
    pub default_water_height: Option<f32>,
    pub location: Option<u32>,
    pub encounter_zone: Option<u32>,
    pub music: Option<u32>,
    pub map_image: Option<String>,
    pub cloud_model: Option<String>,
}

impl World {
    pub fn from_record(rec: &Record, localized: bool) -> Result<World> {
        debug_assert_eq!(rec.tag, tag::WRLD);
        let flags = match rec.find(Tag::new(b"DATA")) {
            None => 0,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"DATA"), d);
                f.exact(1)?;
                f.u8()?
            }
        };
        let parent_flags = match rec.find(Tag::new(b"PNAM")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"PNAM"), d);
                Some(match d.len() {
                    1 => u16::from(f.u8()?),
                    _ => {
                        f.exact(2)?;
                        f.u16()?
                    }
                })
            }
        };
        let center = match rec.find(Tag::new(b"WCTR")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"WCTR"), d);
                f.exact(4)?;
                Some((f.i16()?, f.i16()?))
            }
        };
        let pair = |t: &[u8; 4]| -> Result<Option<(f32, f32)>> {
            match rec.find(Tag::new(t)) {
                None => Ok(None),
                Some(d) => {
                    let mut f = Fields::new(rec.tag, Tag::new(t), d);
                    f.exact(8)?;
                    Ok(Some((f.f32()?, f.f32()?)))
                }
            }
        };
        let map = match rec.find(Tag::new(b"MNAM")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"MNAM"), d);
                if d.len() < 16 {
                    f.exact(16)?;
                }
                Some(MapData {
                    usable_width: f.u32()?,
                    usable_height: f.u32()?,
                    north_west: (f.i16()?, f.i16()?),
                    south_east: (f.i16()?, f.i16()?),
                })
            }
        };
        let (default_land_height, default_water_height) = match rec.find(Tag::new(b"DNAM")) {
            None => (None, None),
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"DNAM"), d);
                f.exact(8)?;
                (Some(f.f32()?), Some(f.f32()?))
            }
        };
        Ok(World {
            form_id: rec.form_id,
            north_rotation: 0.0,
            editor_id: rec.editor_id(),
            name: text(rec, tag::FULL, localized),
            flags,
            parent: form_id(rec, Tag::new(b"WNAM"))?,
            parent_flags,
            climate: form_id(rec, Tag::new(b"CNAM"))?,
            water: form_id(rec, Tag::new(b"NAM2"))?,
            lod_water: form_id(rec, Tag::new(b"NAM3"))?,
            lod_water_height: f32_field(rec, Tag::new(b"NAM4"))?,
            center,
            bounds_min: pair(b"NAM0")?,
            bounds_max: pair(b"NAM9")?,
            map,
            distant_lod_multiplier: f32_field(rec, Tag::new(b"NAMA"))?,
            default_land_height,
            default_water_height,
            location: form_id(rec, Tag::new(b"XLCN"))?,
            encounter_zone: form_id(rec, Tag::new(b"XEZN"))?,
            music: form_id(rec, Tag::new(b"ZNAM"))?,
            map_image: zstring_field(rec, Tag::new(b"TNAM")),
            cloud_model: zstring_field(rec, Tag::new(b"UNAM")),
        })
    }

    pub fn has_landscape(&self) -> bool {
        self.flags & flags::NO_LANDSCAPE == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worldspace_reads_its_extent_and_map() {
        let mut mnam = Vec::new();
        mnam.extend_from_slice(&1024u32.to_le_bytes());
        mnam.extend_from_slice(&768u32.to_le_bytes());
        for v in [-57i16, 50, 61, -43] {
            mnam.extend_from_slice(&v.to_le_bytes());
        }
        let rec = Record::new(tag::WRLD, 0x3C)
            .zsub(tag::EDID, "Tamriel")
            .zsub(tag::FULL, "Skyrim")
            .sub(b"DATA", [flags::NO_GRASS])
            .sub(b"WCTR", [0i16.to_le_bytes(), 0i16.to_le_bytes()].concat())
            .sub(
                b"NAM0",
                [(-64.0f32).to_le_bytes(), (-64.0f32).to_le_bytes()].concat(),
            )
            .sub(
                b"NAM9",
                [64.0f32.to_le_bytes(), 64.0f32.to_le_bytes()].concat(),
            )
            .sub(b"MNAM", mnam)
            .sub(
                b"DNAM",
                [(-27000.0f32).to_le_bytes(), (-14000.0f32).to_le_bytes()].concat(),
            );
        let w = World::from_record(&rec, false).unwrap();
        assert_eq!(w.name.as_ref().unwrap().plain(), Some("Skyrim"));
        assert!(w.has_landscape());
        assert_eq!(w.bounds_min, Some((-64.0, -64.0)));
        assert_eq!(w.map.unwrap().south_east, (61, -43));
        assert_eq!(w.default_water_height, Some(-14000.0));
    }

    #[test]
    fn a_short_map_block_is_an_error() {
        let rec = Record::new(tag::WRLD, 1).sub(b"MNAM", [0u8; 10]);
        assert!(World::from_record(&rec, false).is_err());
    }

    #[test]
    fn worldspace_runtime_north_rotation_starts_at_zero() {
        let rec = Record::new(tag::WRLD, 1);
        let world = World::from_record(&rec, false).unwrap();
        assert_eq!(world.north_rotation, 0.0);
    }
}
