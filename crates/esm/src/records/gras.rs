// SPDX-License-Identifier: GPL-3.0-or-later

//! GRAS: a grass base object. Vanilla never places one through a `REFR`, an
//! `LTEX` names the grasses it can grow through a repeated `GNAM` (see
//! `ltex.rs`), and the engine scatters instances over whichever part of a
//! cell's painted texture layer that `LTEX` names, filtered by this
//! record's density and slope range. Field layout from
//! `TES5Edit/Core/wbDefinitionsTES5.pas` (`wbRecord(GRAS, ...)`), cross
//! checked against `Mutagen.Bethesda.Skyrim/Records/Major Records/Grass.xml`
//! and OpenMW's `components/esm4/loadgras.cpp`, all agreeing on one 32 byte
//! `DATA` block.

use crate::cursor::Fields;
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::zstring_field;

pub mod flags {
    pub const VERTEX_LIGHTING: u8 = 0x01;
    pub const UNIFORM_SCALING: u8 = 0x02;
    pub const FIT_TO_SLOPE: u8 = 0x04;
}

/// `DATA`'s `Units From Water Type` enum: whether `units_from_water` is read
/// as an absolute height, an offset above the water plane, or ignored. The
/// real field is an 8 value enum (`TES5Edit/Core/wbDefinitionsTES5.pas`,
/// `wbRecord(GRAS, ...)`: 0 Above-AtLeast, 1 Above-AtMost, 2 Below-AtLeast,
/// 3 Below-AtMost, 4 Either-AtLeast, 5 Either-AtMost, 6 Either-AtMostAbove,
/// 7 Either-AtMostBelow, cross checked against Mutagen's
/// `Mutagen.Bethesda.Skyrim/Records/Major Records/Grass.cs`
/// `UnitsFromWaterTypeEnum`, same 8 names in the same order). This type
/// collapses that to the 3 states rendering cares about: the 2 above
/// variants, the 2 below variants, and the 4 either variants, which do not
/// restrict by side and so are read as unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterDistanceType {
    AboveWater,
    BelowWater,
    Unused,
}

impl WaterDistanceType {
    fn from_u32(v: u32) -> Self {
        match v {
            0 | 1 => WaterDistanceType::AboveWater,
            2 | 3 => WaterDistanceType::BelowWater,
            _ => WaterDistanceType::Unused,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grass {
    pub form_id: u32,
    pub record_flags: u32,
    pub editor_id: Option<String>,
    /// OBND: min x y z, max x y z, in world units.
    pub bounds: Option<[i16; 6]>,
    /// MODL: the NIF path relative to `Data/Meshes`.
    pub model: Option<String>,
    /// Instances per unit of painted opacity, roughly. CK exposes it as a
    /// single density slider, the exact scatter curve is not published.
    pub density: u8,
    /// Degrees, the terrain slope range this grass is allowed to grow on.
    pub min_slope: u8,
    pub max_slope: u8,
    pub units_from_water: u16,
    pub units_from_water_type: WaterDistanceType,
    /// Fraction a placement may drift from its grid position.
    pub position_range: f32,
    /// Fraction the per-instance scale may vary.
    pub height_range: f32,
    /// Fraction the per-instance vertex colour may vary.
    pub color_range: f32,
    /// Seconds per wind sway cycle, for a shader this engine does not have yet.
    pub wave_period: f32,
    pub flags: u8,
}

impl Grass {
    pub fn from_record(rec: &Record) -> Result<Grass> {
        debug_assert_eq!(rec.tag, tag::GRAS);
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
        let mut density = 0u8;
        let mut min_slope = 0u8;
        let mut max_slope = 90u8;
        let mut units_from_water = 0u16;
        let mut units_from_water_type = WaterDistanceType::AboveWater;
        let mut position_range = 0.0f32;
        let mut height_range = 0.0f32;
        let mut color_range = 0.0f32;
        let mut wave_period = 0.0f32;
        let mut instance_flags = 0u8;
        if let Some(d) = rec.find(Tag::new(b"DATA")) {
            let mut f = Fields::new(rec.tag, Tag::new(b"DATA"), d);
            f.exact(32)?;
            density = f.u8()?;
            min_slope = f.u8()?;
            max_slope = f.u8()?;
            f.skip(1)?;
            units_from_water = f.u16()?;
            f.skip(2)?;
            units_from_water_type = WaterDistanceType::from_u32(f.u32()?);
            position_range = f.f32()?;
            height_range = f.f32()?;
            color_range = f.f32()?;
            wave_period = f.f32()?;
            instance_flags = f.u8()?;
            f.skip(3)?;
        }
        Ok(Grass {
            form_id: rec.form_id,
            record_flags: rec.flags,
            editor_id: rec.editor_id(),
            bounds,
            model: zstring_field(rec, Tag::new(b"MODL")),
            density,
            min_slope,
            max_slope,
            units_from_water,
            units_from_water_type,
            position_range,
            height_range,
            color_range,
            wave_period,
            flags: instance_flags,
        })
    }

    pub fn fits_slope(&self, degrees: f32) -> bool {
        degrees >= f32::from(self.min_slope) && degrees <= f32::from(self.max_slope)
    }
}
