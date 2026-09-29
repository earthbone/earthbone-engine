// SPDX-License-Identifier: GPL-3.0-or-later

//! CELL: an interior cell or one exterior grid square of a worldspace.

use crate::cursor::Fields;
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::{f32_field, form_id, form_id_list, text, Text};

pub mod flags {
    pub const INTERIOR: u16 = 0x0001;
    pub const HAS_WATER: u16 = 0x0002;
    pub const CANT_TRAVEL_FROM_HERE: u16 = 0x0004;
    pub const NO_LOD_WATER: u16 = 0x0008;
    pub const PUBLIC: u16 = 0x0020;
    pub const HAND_CHANGED: u16 = 0x0040;
    pub const SHOW_SKY: u16 = 0x0080;
    pub const USE_SKY_LIGHTING: u16 = 0x0100;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub x: i32,
    pub y: i32,
    /// Force-hide land quadrant bits, when the subrecord carries them.
    pub flags: Option<u32>,
}

/// A four-byte color in a CELL XCLL record, including its unused fourth byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightingColor(pub [u8; 4]);

/// Decoded CELL XCLL lighting. Float values are encoded from their original
/// IEEE-754 values, preserving every source bit pattern on a decode/encode
/// round trip.
#[derive(Debug, Clone, PartialEq)]
pub struct Lighting {
    pub ambient_color: LightingColor,
    pub directional_color: LightingColor,
    pub fog_near_color: LightingColor,
    pub fog_near: f32,
    pub fog_far: f32,
    pub directional_rotation_xy: i32,
    pub directional_rotation_z: i32,
    pub directional_fade: f32,
    pub fog_clip_distance: f32,
    pub fog_power: f32,
    pub ambient_colors: [LightingColor; 6],
    /// Optional in the TES5 schema.
    pub specular_color: Option<LightingColor>,
    pub fresnel_power: Option<f32>,
    pub fog_far_color: Option<LightingColor>,
    pub fog_max: Option<f32>,
    pub light_fade_begin: Option<f32>,
    pub light_fade_end: Option<f32>,
    pub inheritance_flags: Option<u32>,
}

impl Lighting {
    /// Decodes XCLL with the optional trailing fields defined by xEdit.
    ///
    /// # Errors
    ///
    /// Returns an error if the field is too short, too long or ends inside a
    /// trailing field.
    pub fn decode(data: &[u8]) -> crate::error::Result<Self> {
        let tag = Tag::new(b"XCLL");
        if !(64..=92).contains(&data.len()) || !(data.len() - 64).is_multiple_of(4) {
            return Err(crate::error::Error::SubrecordSize {
                record: tag::CELL,
                tag,
                len: data.len(),
                expected: "64 to 92 bytes, with complete 4-byte trailing fields".into(),
            });
        }
        let mut f = Fields::new(tag::CELL, tag, data);
        let color = |f: &mut Fields<'_>| -> crate::error::Result<LightingColor> {
            Ok(LightingColor([f.u8()?, f.u8()?, f.u8()?, f.u8()?]))
        };
        let ambient_color = color(&mut f)?;
        let directional_color = color(&mut f)?;
        let fog_near_color = color(&mut f)?;
        let fog_near = f.f32()?;
        let fog_far = f.f32()?;
        let directional_rotation_xy = f.i32()?;
        let directional_rotation_z = f.i32()?;
        let directional_fade = f.f32()?;
        let fog_clip_distance = f.f32()?;
        let fog_power = f.f32()?;
        let ambient_colors = [
            color(&mut f)?,
            color(&mut f)?,
            color(&mut f)?,
            color(&mut f)?,
            color(&mut f)?,
            color(&mut f)?,
        ];
        let specular_color = (f.remaining() >= 4).then(|| color(&mut f)).transpose()?;
        let fresnel_power = (f.remaining() >= 4).then(|| f.f32()).transpose()?;
        let fog_far_color = (f.remaining() >= 4).then(|| color(&mut f)).transpose()?;
        let fog_max = (f.remaining() >= 4).then(|| f.f32()).transpose()?;
        let light_fade_begin = (f.remaining() >= 4).then(|| f.f32()).transpose()?;
        let light_fade_end = (f.remaining() >= 4).then(|| f.f32()).transpose()?;
        let inheritance_flags = (f.remaining() >= 4).then(|| f.u32()).transpose()?;
        Ok(Self {
            ambient_color,
            directional_color,
            fog_near_color,
            fog_near,
            fog_far,
            directional_rotation_xy,
            directional_rotation_z,
            directional_fade,
            fog_clip_distance,
            fog_power,
            ambient_colors,
            specular_color,
            fresnel_power,
            fog_far_color,
            fog_max,
            light_fade_begin,
            light_fade_end,
            inheritance_flags,
        })
    }

    /// Encodes the lighting structure with its present trailing fields.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(92);
        bytes.extend_from_slice(&self.ambient_color.0);
        bytes.extend_from_slice(&self.directional_color.0);
        bytes.extend_from_slice(&self.fog_near_color.0);
        bytes.extend_from_slice(&self.fog_near.to_bits().to_le_bytes());
        bytes.extend_from_slice(&self.fog_far.to_bits().to_le_bytes());
        bytes.extend_from_slice(&self.directional_rotation_xy.to_le_bytes());
        bytes.extend_from_slice(&self.directional_rotation_z.to_le_bytes());
        bytes.extend_from_slice(&self.directional_fade.to_bits().to_le_bytes());
        bytes.extend_from_slice(&self.fog_clip_distance.to_bits().to_le_bytes());
        bytes.extend_from_slice(&self.fog_power.to_bits().to_le_bytes());
        for value in self.ambient_colors {
            bytes.extend_from_slice(&value.0);
        }
        if let Some(value) = self.specular_color {
            bytes.extend_from_slice(&value.0);
        }
        if let Some(value) = self.fresnel_power {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        if let Some(value) = self.fog_far_color {
            bytes.extend_from_slice(&value.0);
        }
        if let Some(value) = self.fog_max {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        if let Some(value) = self.light_fade_begin {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        if let Some(value) = self.light_fade_end {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        if let Some(value) = self.inheritance_flags {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub flags: u16,
    /// Present for exterior cells.
    pub grid: Option<Grid>,
    /// Cell lighting from XCLL, when present.
    pub lighting: Option<Lighting>,
    pub lighting_template: Option<u32>,
    pub water_height: Option<f32>,
    pub water: Option<u32>,
    pub owner: Option<u32>,
    pub location: Option<u32>,
    pub regions: Vec<u32>,
    pub image_space: Option<u32>,
    pub music: Option<u32>,
    pub acoustic_space: Option<u32>,
    pub encounter_zone: Option<u32>,
    pub sky_and_weather: Option<u32>,
    /// The enclosing worldspace, from the group tree.
    pub world: Option<u32>,
}

impl Cell {
    pub fn from_record(rec: &Record, localized: bool) -> Result<Cell> {
        debug_assert_eq!(rec.tag, tag::CELL);
        let lighting = match rec.all(Tag::new(b"XCLL")).collect::<Vec<_>>().as_slice() {
            [] => None,
            [data] => Some(Lighting::decode(data)?),
            _ => {
                return Err(crate::error::Error::Invalid(
                    "CELL contains repeated XCLL lighting subrecords".into(),
                ));
            }
        };
        let data = rec.required(Tag::new(b"DATA"))?;
        let mut f = Fields::new(rec.tag, Tag::new(b"DATA"), data);
        let flags = match data.len() {
            1 => u16::from(f.u8()?),
            _ => {
                f.exact(2)?;
                f.u16()?
            }
        };
        let grid = match rec.find(Tag::new(b"XCLC")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"XCLC"), d);
                let x = f.i32()?;
                let y = f.i32()?;
                let flags = if f.remaining() >= 4 {
                    Some(f.u32()?)
                } else {
                    None
                };
                Some(Grid { x, y, flags })
            }
        };
        Ok(Cell {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: text(rec, tag::FULL, localized),
            flags,
            grid,
            lighting,
            lighting_template: form_id(rec, Tag::new(b"LTMP"))?,
            water_height: f32_field(rec, Tag::new(b"XCLW"))?,
            water: form_id(rec, Tag::new(b"XCWT"))?,
            owner: form_id(rec, Tag::new(b"XOWN"))?,
            location: form_id(rec, Tag::new(b"XLCN"))?,
            regions: form_id_list(rec, Tag::new(b"XCLR"))?,
            image_space: form_id(rec, Tag::new(b"XCIM"))?,
            music: form_id(rec, Tag::new(b"XCMO"))?,
            acoustic_space: form_id(rec, Tag::new(b"XCAS"))?,
            encounter_zone: form_id(rec, Tag::new(b"XEZN"))?,
            sky_and_weather: form_id(rec, Tag::new(b"XCCM"))?,
            world: rec.placement.world,
        })
    }

    pub fn is_interior(&self) -> bool {
        self.flags & flags::INTERIOR != 0
    }

    pub fn has_water(&self) -> bool {
        self.flags & flags::HAS_WATER != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exterior_cell_reads_its_grid_and_water() {
        let mut xclc = Vec::new();
        xclc.extend_from_slice(&5i32.to_le_bytes());
        xclc.extend_from_slice(&(-3i32).to_le_bytes());
        xclc.extend_from_slice(&0u32.to_le_bytes());
        let rec = Record::new(tag::CELL, 0x1234)
            .zsub(tag::EDID, "WhiterunExterior01")
            .sub(b"DATA", [flags::HAS_WATER as u8, 0])
            .sub(b"XCLC", xclc)
            .sub(b"XCLW", (-14000.0f32).to_le_bytes())
            .sub(
                b"XCLR",
                [0x11u32.to_le_bytes(), 0x22u32.to_le_bytes()].concat(),
            );
        let c = Cell::from_record(&rec, false).unwrap();
        assert!(!c.is_interior());
        assert!(c.has_water());
        assert_eq!(
            c.grid,
            Some(Grid {
                x: 5,
                y: -3,
                flags: Some(0)
            })
        );
        assert_eq!(c.water_height, Some(-14000.0));
        assert_eq!(c.regions, vec![0x11, 0x22]);
    }

    #[test]
    fn a_localized_name_is_kept_as_an_id() {
        let rec = Record::new(tag::CELL, 1)
            .sub(b"DATA", [flags::INTERIOR as u8])
            .sub(tag::FULL, 77u32.to_le_bytes());
        let c = Cell::from_record(&rec, true).unwrap();
        assert!(c.is_interior());
        assert_eq!(c.name, Some(Text::Localized(77)));
        let c = Cell::from_record(&rec, false).unwrap();
        assert!(matches!(c.name, Some(Text::Plain(_))));
    }

    #[test]
    fn a_cell_without_data_is_an_error() {
        let rec = Record::new(tag::CELL, 1);
        assert!(Cell::from_record(&rec, false).is_err());
        let rec = Record::new(tag::CELL, 1).sub(b"DATA", [1, 2, 3]);
        assert!(Cell::from_record(&rec, false).is_err());
    }
}
