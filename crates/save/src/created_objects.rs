// SPDX-License-Identifier: GPL-3.0-or-later

//! Created Objects, global data type 4. Field order follows UESP
//! Created_Objects and skymp's GPLv3 SFReader/SFWriter implementations.
//! These tables describe saved magic effects, not complete plugin records.

use crate::cursor::{Cursor, VsVal};
use crate::{Error, GlobalData, RefId, Result, Save};

/// One effect in a saved created enchantment, potion or poison.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatedEffect {
    pub effect: RefId,
    pub magnitude: f32,
    /// The save stores duration before area, unlike a plugin EFIT subrecord.
    pub duration: u32,
    pub area: u32,
    /// Preserved as stored. UESP calls this the effect's price contribution.
    pub price: f32,
}

/// One saved created magic item. Identity resolution is a separate step.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatedObject {
    pub form: RefId,
    /// UESP and skymp name this timesUsed. Its runtime meaning is uncertain.
    pub times_used: u32,
    pub effects: Vec<CreatedEffect>,
    effect_count: VsVal,
}

/// The four tables in their source-defined order, with original count widths.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatedObjects {
    pub weapon_enchantments: Vec<CreatedObject>,
    pub armor_enchantments: Vec<CreatedObject>,
    pub potions: Vec<CreatedObject>,
    pub poisons: Vec<CreatedObject>,
    counts: [VsVal; 4],
}

fn bad(reason: impl Into<String>) -> Error {
    Error::CreatedObjects {
        reason: reason.into(),
    }
}

fn read_table(r: &mut Cursor<'_>) -> Result<(VsVal, Vec<CreatedObject>)> {
    let count = r.vsval()?;
    // Three RefID bytes, a u32 and at least one effect-count byte per item.
    if count.value() as usize > r.remaining() / 8 {
        return Err(bad(format!(
            "object count {} exceeds remaining bytes at {}",
            count.value(),
            r.pos()
        )));
    }
    let mut objects = Vec::with_capacity(count.value() as usize);
    for _ in 0..count.value() {
        let form = r.ref_id()?;
        let times_used = r.u32()?;
        let effect_count = r.vsval()?;
        // RefID, magnitude, duration, area and price occupy exactly 19 bytes.
        if effect_count.value() as usize > r.remaining() / 19 {
            return Err(bad(format!(
                "effect count {} exceeds remaining bytes at {}",
                effect_count.value(),
                r.pos()
            )));
        }
        let mut effects = Vec::with_capacity(effect_count.value() as usize);
        for _ in 0..effect_count.value() {
            effects.push(CreatedEffect {
                effect: r.ref_id()?,
                magnitude: r.f32()?,
                duration: r.u32()?,
                area: r.u32()?,
                price: r.f32()?,
            });
        }
        objects.push(CreatedObject {
            form,
            times_used,
            effects,
            effect_count,
        });
    }
    Ok((count, objects))
}

impl CreatedObjects {
    /// Decode exactly one type-4 payload, bounding allocations by input size.
    /// Reference identities and float bits are retained without normalization.
    ///
    /// # Errors
    /// Truncated fields, invalid count widths, impossible counts or extra bytes.
    pub fn read(data: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(data, "created objects");
        let (weapon_count, weapon_enchantments) = read_table(&mut r)?;
        let (armor_count, armor_enchantments) = read_table(&mut r)?;
        let (potion_count, potions) = read_table(&mut r)?;
        let (poison_count, poisons) = read_table(&mut r)?;
        if !r.is_empty() {
            return Err(bad(format!("trailing bytes at {}", r.pos())));
        }
        Ok(Self {
            weapon_enchantments,
            armor_enchantments,
            potions,
            poisons,
            counts: [weapon_count, armor_count, potion_count, poison_count],
        })
    }

    /// Rebuild the complete payload from typed fields, retaining count widths.
    ///
    /// # Errors
    /// A table or effect list was resized without its saved count matching.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for (count, objects) in self.counts.iter().zip([
            &self.weapon_enchantments,
            &self.armor_enchantments,
            &self.potions,
            &self.poisons,
        ]) {
            if count.value() as usize != objects.len() {
                return Err(bad("object count does not match table length"));
            }
            count.write(&mut out);
            for object in objects {
                if object.effect_count.value() as usize != object.effects.len() {
                    return Err(bad("effect count does not match list length"));
                }
                object.form.write(&mut out);
                out.extend_from_slice(&object.times_used.to_le_bytes());
                object.effect_count.write(&mut out);
                for effect in &object.effects {
                    effect.effect.write(&mut out);
                    out.extend_from_slice(&effect.magnitude.to_le_bytes());
                    out.extend_from_slice(&effect.duration.to_le_bytes());
                    out.extend_from_slice(&effect.area.to_le_bytes());
                    out.extend_from_slice(&effect.price.to_le_bytes());
                }
            }
        }
        Ok(out)
    }
}

impl Save {
    /// Decode the optional type-4 Created Objects block.
    ///
    /// # Errors
    /// Duplicate blocks or an invalid payload.
    pub fn created_objects(&self) -> Result<Option<CreatedObjects>> {
        let mut blocks = self
            .table_1
            .iter()
            .filter(|block| block.kind == GlobalData::CREATED_OBJECTS);
        let first = blocks.next();
        if blocks.next().is_some() {
            return Err(bad("duplicate Created Objects block"));
        }
        first
            .map(|block| CreatedObjects::read(&block.data))
            .transpose()
    }
}
