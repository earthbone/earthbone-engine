// SPDX-License-Identifier: GPL-3.0-or-later

//! LAND: one exterior cell's terrain, a 33 by 33 vertex grid.
//!
//! Heights are stored as a float offset plus one signed byte per vertex.
//! The bytes are deltas: the first vertex of each row is relative to the
//! first vertex of the row below, every other vertex is relative to the one
//! to its left, and the sum is scaled by eight into world units. Normals and
//! colours are three bytes per vertex. Textures are per quadrant, one base
//! plus up to eight alpha layers over a 17 by 17 grid.

use crate::cursor::Fields;
use crate::error::{Error, Result};
use crate::plugin::Record;
use crate::tag::{self, Tag};

pub const VERTS_PER_SIDE: usize = 33;
pub const VERTS: usize = VERTS_PER_SIDE * VERTS_PER_SIDE;
pub const ALPHA_GRID_SIDE: usize = 17;
pub const HEIGHT_SCALE: f32 = 8.0;

pub mod flags {
    pub const HAS_NORMALS_AND_HEIGHTS: u32 = 0x001;
    pub const HAS_VERTEX_COLOURS: u32 = 0x002;
    pub const HAS_LAYERS: u32 = 0x004;
    pub const AUTO_CALC_NORMALS: u32 = 0x010;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heights {
    pub offset: f32,
    /// Row-major, 33 rows of 33 deltas, row 0 at the south edge.
    pub gradients: Vec<i8>,
}

impl Heights {
    /// Absolute heights in world units, row-major, 1089 values.
    pub fn decode(&self) -> Vec<f32> {
        let mut out = vec![0f32; VERTS];
        let mut row_start = self.offset;
        for row in 0..VERTS_PER_SIDE {
            let mut current = 0f32;
            for col in 0..VERTS_PER_SIDE {
                let g = f32::from(self.gradients[row * VERTS_PER_SIDE + col]);
                if col == 0 {
                    row_start += g;
                    current = row_start;
                } else {
                    current += g;
                }
                out[row * VERTS_PER_SIDE + col] = current * HEIGHT_SCALE;
            }
        }
        out
    }

    /// Encode absolute heights into the gradient form. The input is in world
    /// units and is quantised to the byte grid, so decode(encode(h)) is
    /// within 8 units of h when the slopes fit in a signed byte.
    pub fn encode(heights: &[f32]) -> Heights {
        assert_eq!(heights.len(), VERTS);
        let q = |v: f32| (v / HEIGHT_SCALE).round();
        let offset = q(heights[0]);
        let mut gradients = vec![0i8; VERTS];
        let mut row_start = offset;
        for row in 0..VERTS_PER_SIDE {
            let mut prev = 0f32;
            for col in 0..VERTS_PER_SIDE {
                let target = q(heights[row * VERTS_PER_SIDE + col]);
                let delta = if col == 0 {
                    let d = target - row_start;
                    row_start = target;
                    d
                } else {
                    target - prev
                };
                prev = target;
                gradients[row * VERTS_PER_SIDE + col] = delta.clamp(-128.0, 127.0) as i8;
            }
        }
        // The first vertex's delta is folded into the offset.
        gradients[0] = 0;
        Heights { offset, gradients }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + VERTS + 3);
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend(self.gradients.iter().map(|g| *g as u8));
        out.extend_from_slice(&[0, 0, 0]);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alpha {
    /// Index into the 17 by 17 quadrant grid.
    pub position: u16,
    pub opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// LTEX FormID.
    pub texture: u32,
    pub index: u16,
    pub alphas: Vec<Alpha>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quadrant {
    /// LTEX FormID of the base texture, zero for the default.
    pub base: Option<u32>,
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Land {
    pub form_id: u32,
    pub flags: u32,
    pub heights: Option<Heights>,
    /// Per vertex X Y Z, row-major.
    pub normals: Option<Vec<[i8; 3]>>,
    pub colours: Option<Vec<[u8; 3]>>,
    /// Bottom left, bottom right, top left, top right.
    pub quadrants: [Quadrant; 4],
    pub cell: Option<u32>,
    pub world: Option<u32>,
}

impl Land {
    pub fn from_record(rec: &Record) -> Result<Land> {
        debug_assert_eq!(rec.tag, tag::LAND);
        let flags = match rec.find(Tag::new(b"DATA")) {
            None => 0,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"DATA"), d);
                f.exact(4)?;
                f.u32()?
            }
        };
        let heights = match rec.find(Tag::new(b"VHGT")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"VHGT"), d);
                f.exact(4 + VERTS + 3)?;
                let offset = f.f32()?;
                let gradients = f.bytes(VERTS)?.iter().map(|b| *b as i8).collect();
                Some(Heights { offset, gradients })
            }
        };
        let triples = |t: &[u8; 4]| -> Result<Option<Vec<[u8; 3]>>> {
            match rec.find(Tag::new(t)) {
                None => Ok(None),
                Some(d) => {
                    let f = Fields::new(rec.tag, Tag::new(t), d);
                    f.exact(VERTS * 3)?;
                    Ok(Some(d.as_chunks::<3>().0.to_vec()))
                }
            }
        };
        let normals = triples(b"VNML")?.map(|v| {
            v.into_iter()
                .map(|c| [c[0] as i8, c[1] as i8, c[2] as i8])
                .collect()
        });
        let colours = triples(b"VCLR")?;

        let mut quadrants: [Quadrant; 4] = Default::default();
        let mut open: Option<(usize, Layer)> = None;
        for s in &rec.subrecords {
            match s.tag.as_bytes() {
                b"BTXT" => {
                    let mut f = Fields::new(rec.tag, s.tag, &s.data);
                    f.exact(8)?;
                    let texture = f.u32()?;
                    let quadrant = f.u8()? as usize;
                    if quadrant >= 4 {
                        return Err(Error::Invalid(format!(
                            "LAND {:08X}: base texture quadrant {quadrant} out of range",
                            rec.form_id
                        )));
                    }
                    quadrants[quadrant].base = Some(texture);
                }
                b"ATXT" => {
                    if let Some((q, layer)) = open.take() {
                        quadrants[q].layers.push(layer);
                    }
                    let mut f = Fields::new(rec.tag, s.tag, &s.data);
                    f.exact(8)?;
                    let texture = f.u32()?;
                    let quadrant = f.u8()? as usize;
                    f.skip(1)?;
                    let index = f.u16()?;
                    if quadrant >= 4 {
                        return Err(Error::Invalid(format!(
                            "LAND {:08X}: layer texture quadrant {quadrant} out of range",
                            rec.form_id
                        )));
                    }
                    open = Some((
                        quadrant,
                        Layer {
                            texture,
                            index,
                            alphas: Vec::new(),
                        },
                    ));
                }
                b"VTXT" => {
                    let Some((_, layer)) = open.as_mut() else {
                        return Err(Error::Invalid(format!(
                            "LAND {:08X}: VTXT alpha data without a preceding ATXT",
                            rec.form_id
                        )));
                    };
                    if s.data.len() % 8 != 0 {
                        return Err(Error::SubrecordSize {
                            record: rec.tag,
                            tag: s.tag,
                            len: s.data.len(),
                            expected: "a multiple of 8".into(),
                        });
                    }
                    let mut f = Fields::new(rec.tag, s.tag, &s.data);
                    while f.remaining() >= 8 {
                        let position = f.u16()?;
                        f.skip(2)?;
                        let opacity = f.f32()?;
                        if usize::from(position) >= ALPHA_GRID_SIDE * ALPHA_GRID_SIDE {
                            return Err(Error::Invalid(format!(
                                "LAND {:08X}: alpha position {position} outside the 17 by 17 grid",
                                rec.form_id
                            )));
                        }
                        layer.alphas.push(Alpha { position, opacity });
                    }
                }
                _ => {}
            }
        }
        if let Some((q, layer)) = open.take() {
            quadrants[q].layers.push(layer);
        }
        Ok(Land {
            form_id: rec.form_id,
            flags,
            heights,
            normals,
            colours,
            quadrants,
            cell: rec.placement.cell,
            world: rec.placement.world,
        })
    }
}

/// Apply one circular height brush dab to a LAND record and rebuild its
/// vertex normals from the edited heights.
///
/// `center` and `radius` use cell-local world units. `strength` is the height
/// change at the brush centre. A smooth radial falloff reaches zero at the
/// radius. Existing LAND subrecords and unknown data are preserved.
pub fn apply_height_brush(
    record: &mut Record,
    center: [f32; 2],
    radius: f32,
    strength: f32,
) -> Result<()> {
    if record.tag != tag::LAND {
        return Err(Error::Invalid("height brush requires a LAND record".into()));
    }
    if !center.into_iter().all(f32::is_finite)
        || !radius.is_finite()
        || radius <= 0.0
        || !strength.is_finite()
    {
        return Err(Error::Invalid(
            "height brush center, radius and strength must be finite, with a positive radius"
                .into(),
        ));
    }

    let land = Land::from_record(record)?;
    for subrecord_tag in [b"DATA", b"VHGT", b"VNML"] {
        ensure_unique_subrecord(record, Tag::new(subrecord_tag))?;
    }
    let mut heights = land
        .heights
        .map(|values| values.decode())
        .unwrap_or_else(|| vec![0.0; VERTS]);
    let spacing = 4096.0 / (VERTS_PER_SIDE - 1) as f32;
    for row in 0..VERTS_PER_SIDE {
        for col in 0..VERTS_PER_SIDE {
            let x = col as f32 * spacing;
            let y = row as f32 * spacing;
            let distance = (x - center[0]).hypot(y - center[1]);
            if distance < radius {
                let t = 1.0 - distance / radius;
                let weight = t * t * (3.0 - 2.0 * t);
                heights[row * VERTS_PER_SIDE + col] += strength * weight;
            }
        }
    }

    ensure_encodable_heights(&heights)?;
    let encoded = Heights::encode(&heights);
    let mut normals = Vec::with_capacity(VERTS * 3);
    for row in 0..VERTS_PER_SIDE {
        for col in 0..VERTS_PER_SIDE {
            let left = row * VERTS_PER_SIDE + col.saturating_sub(1);
            let right = row * VERTS_PER_SIDE + (col + 1).min(VERTS_PER_SIDE - 1);
            let south = row.saturating_sub(1) * VERTS_PER_SIDE + col;
            let north = (row + 1).min(VERTS_PER_SIDE - 1) * VERTS_PER_SIDE + col;
            let dx = (heights[right] - heights[left])
                / (spacing
                    * if col == 0 || col == VERTS_PER_SIDE - 1 {
                        1.0
                    } else {
                        2.0
                    });
            let dy = (heights[north] - heights[south])
                / (spacing
                    * if row == 0 || row == VERTS_PER_SIDE - 1 {
                        1.0
                    } else {
                        2.0
                    });
            let length = (dx * dx + dy * dy + 1.0).sqrt();
            for component in [-dx / length, -dy / length, 1.0 / length] {
                normals.push((component * 127.0).round().clamp(-127.0, 127.0) as i8 as u8);
            }
        }
    }

    let flags = land.flags | flags::HAS_NORMALS_AND_HEIGHTS;
    if record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.tag == Tag::new(b"DATA"))
        .is_some()
    {
        let data = record
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.tag == Tag::new(b"DATA"));
        if let Some(data) = data {
            data.data = flags.to_le_bytes().to_vec();
        }
    } else {
        record.subrecords.insert(
            0,
            crate::plugin::Subrecord::new(Tag::new(b"DATA"), flags.to_le_bytes()),
        );
    }
    set_unique_subrecord(record, Tag::new(b"VHGT"), encoded.to_bytes())?;
    set_unique_subrecord(record, Tag::new(b"VNML"), normals)?;
    Ok(())
}

fn ensure_encodable_heights(heights: &[f32]) -> Result<()> {
    let quantized = heights
        .iter()
        .map(|height| (*height / HEIGHT_SCALE).round())
        .collect::<Vec<_>>();
    if quantized.iter().any(|height| !height.is_finite()) {
        return Err(Error::Invalid(
            "height brush produced a non-finite LAND height".into(),
        ));
    }
    let mut row_start = quantized[0];
    for row in 0..VERTS_PER_SIDE {
        let mut previous = 0.0;
        for col in 0..VERTS_PER_SIDE {
            let target = quantized[row * VERTS_PER_SIDE + col];
            let delta = if col == 0 {
                let delta = target - row_start;
                row_start = target;
                delta
            } else {
                target - previous
            };
            if !(-128.0..=127.0).contains(&delta) {
                return Err(Error::Invalid(
                    "height brush exceeds LAND gradient range".into(),
                ));
            }
            previous = target;
        }
    }
    Ok(())
}

fn set_unique_subrecord(record: &mut Record, tag: Tag, data: Vec<u8>) -> Result<()> {
    let mut found = record
        .subrecords
        .iter_mut()
        .filter(|subrecord| subrecord.tag == tag);
    if let Some(subrecord) = found.next() {
        if found.next().is_some() {
            return Err(Error::Invalid(format!(
                "LAND has repeated {tag} subrecords"
            )));
        }
        subrecord.data = data;
    } else {
        record
            .subrecords
            .push(crate::plugin::Subrecord::new(tag, data));
    }
    Ok(())
}

fn ensure_unique_subrecord(record: &Record, tag: Tag) -> Result<()> {
    if record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.tag == tag)
        .count()
        > 1
    {
        return Err(Error::Invalid(format!(
            "LAND has repeated {tag} subrecords"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slope() -> Vec<f32> {
        (0..VERTS)
            .map(|i| {
                let row = (i / VERTS_PER_SIDE) as f32;
                let col = (i % VERTS_PER_SIDE) as f32;
                -1000.0 + row * 16.0 + col * 8.0
            })
            .collect()
    }

    #[test]
    fn heights_round_trip_through_the_gradient_form() {
        let h = slope();
        let enc = Heights::encode(&h);
        let dec = enc.decode();
        for (a, b) in h.iter().zip(&dec) {
            assert!((a - b).abs() <= 8.0, "{a} vs {b}");
        }
        // A hand-computed corner: offset -125 (times 8 = -1000), row 1 col 0
        // is one step of 16 units, so gradient 2.
        assert_eq!(enc.offset, -125.0);
        assert_eq!(enc.gradients[VERTS_PER_SIDE], 2);
        assert_eq!(enc.gradients[1], 1);
        assert_eq!(dec[0], -1000.0);
        assert_eq!(dec[VERTS_PER_SIDE], -984.0);
        assert_eq!(dec[VERTS_PER_SIDE + 1], -976.0);
    }

    #[test]
    fn a_full_land_record_decodes_layers_in_order() {
        let h = Heights::encode(&slope());
        let mut btxt = 0x100u32.to_le_bytes().to_vec();
        btxt.extend_from_slice(&[2, 0, 0xFF, 0xFF]);
        let mut atxt = 0x200u32.to_le_bytes().to_vec();
        atxt.extend_from_slice(&[2, 0, 0, 0]);
        let mut vtxt = Vec::new();
        for (pos, op) in [(0u16, 1.0f32), (17, 0.5)] {
            vtxt.extend_from_slice(&pos.to_le_bytes());
            vtxt.extend_from_slice(&[0, 0]);
            vtxt.extend_from_slice(&op.to_le_bytes());
        }
        let mut atxt2 = 0x300u32.to_le_bytes().to_vec();
        atxt2.extend_from_slice(&[1, 0, 0, 0]);
        let rec = Record::new(tag::LAND, 0x5000)
            .sub(
                b"DATA",
                (flags::HAS_NORMALS_AND_HEIGHTS | flags::HAS_LAYERS).to_le_bytes(),
            )
            .sub(b"VNML", vec![0u8; VERTS * 3])
            .sub(b"VHGT", h.to_bytes())
            .sub(b"BTXT", btxt)
            .sub(b"ATXT", atxt)
            .sub(b"VTXT", vtxt)
            .sub(b"ATXT", atxt2);
        let land = Land::from_record(&rec).unwrap();
        assert_eq!(land.heights, Some(h));
        assert_eq!(land.normals.as_ref().unwrap().len(), VERTS);
        assert_eq!(land.quadrants[2].base, Some(0x100));
        assert_eq!(land.quadrants[2].layers.len(), 1);
        assert_eq!(
            land.quadrants[2].layers[0].alphas[1],
            Alpha {
                position: 17,
                opacity: 0.5
            }
        );
        assert_eq!(land.quadrants[1].layers[0].texture, 0x300);
        assert!(land.quadrants[1].layers[0].alphas.is_empty());
    }

    #[test]
    fn malformed_land_is_refused_with_a_reason() {
        let rec = Record::new(tag::LAND, 1).sub(b"VHGT", vec![0u8; 100]);
        assert!(matches!(
            Land::from_record(&rec).unwrap_err(),
            Error::SubrecordSize { .. }
        ));
        let rec = Record::new(tag::LAND, 1).sub(b"VTXT", vec![0u8; 8]);
        assert!(Land::from_record(&rec)
            .unwrap_err()
            .to_string()
            .contains("without a preceding ATXT"));
        let mut btxt = 0u32.to_le_bytes().to_vec();
        btxt.extend_from_slice(&[9, 0, 0, 0]);
        let rec = Record::new(tag::LAND, 1).sub(b"BTXT", btxt);
        assert!(Land::from_record(&rec)
            .unwrap_err()
            .to_string()
            .contains("quadrant 9"));
    }
}
