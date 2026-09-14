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

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub flags: u16,
    /// Present for exterior cells.
    pub grid: Option<Grid>,
    /// XCLL as stored, 92 bytes in SE. Decoding it is the renderer's job.
    pub lighting: Option<Vec<u8>>,
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
            lighting: rec.find(Tag::new(b"XCLL")).map(<[u8]>::to_vec),
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
