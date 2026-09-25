// SPDX-License-Identifier: GPL-3.0-or-later

//! Water type shading data, read straight from a WATR record's raw
//! subrecords. No `esm` crate module exists for WATR yet, so this follows
//! the generic `Record::find` pattern `models::model_for` already uses for
//! FURN and DOOR, instead of adding a dedicated record view for one field
//! set.
//!
//! Layout is TES5Edit's `wbDefinitionsTES5.pas` (WATR, lines 10304 to
//! 10429) and the UESP WATR page: ANAM is a single opacity byte, and DNAM
//! is a 228 (base game) or 232 (Special Edition) byte block whose Shallow,
//! Deep and Reflection colors sit at byte offsets 40, 44 and 48, each a
//! four byte R, G, B, unused `wbByteColors` struct. TNAM (the water's MATT
//! material) and DNAM's wave, flow and fog parameters are not read here,
//! they have no effect while materials render unlit. See
//! `docs/godot-host.md` for what is deferred.

use soe_esm::cursor::{zstring, Fields};
use soe_esm::{Record, Tag};

use crate::mesh::WaterMaterial;

const SHALLOW_COLOR_OFFSET: usize = 40;
const REFLECTION_COLOR_OFFSET: usize = 48;

/// Reads a WATR record's opacity, its three DNAM colors and its first
/// noise texture layer. Always returns a value, missing or malformed
/// subrecords fall back to full opacity and black colors rather than
/// failing the cell's water plane.
pub(crate) fn water_material_from(record: &Record) -> WaterMaterial {
    let opacity = record
        .find(Tag::new(b"ANAM"))
        .and_then(|d| d.first().copied())
        .unwrap_or(255);
    let (shallow_color, deep_color, reflection_color) = record
        .find(Tag::new(b"DNAM"))
        .and_then(read_dnam_colors)
        .unwrap_or_default();
    let noise_texture = record
        .find(Tag::new(b"NAM2"))
        .map(zstring)
        .filter(|s| !s.is_empty());
    WaterMaterial {
        opacity,
        shallow_color,
        deep_color,
        reflection_color,
        noise_texture,
    }
}

type Colors = ([f32; 3], [f32; 3], [f32; 3]);

/// The Shallow, Deep and Reflection colors from DNAM, each a byte RGB
/// triple normalised to 0.0 to 1.0. `None` for a DNAM shorter than the
/// reflection color's offset, which no vanilla or DLC record produces but
/// a malformed plugin could.
fn read_dnam_colors(dnam: &[u8]) -> Option<Colors> {
    if dnam.len() < REFLECTION_COLOR_OFFSET + 4 {
        return None;
    }
    let mut f = Fields::new(Tag::new(b"WATR"), Tag::new(b"DNAM"), dnam);
    f.skip(SHALLOW_COLOR_OFFSET).ok()?;
    let shallow = read_color(&mut f)?;
    let deep = read_color(&mut f)?;
    let reflection = read_color(&mut f)?;
    Some((shallow, deep, reflection))
}

/// One `wbByteColors`: R, G, B, then one unused byte.
fn read_color(f: &mut Fields<'_>) -> Option<[f32; 3]> {
    let r = f.u8().ok()?;
    let g = f.u8().ok()?;
    let b = f.u8().ok()?;
    f.skip(1).ok()?;
    Some([
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ])
}
