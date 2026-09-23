// SPDX-License-Identifier: GPL-3.0-or-later

//! RACE attribute prefix from xEdit wbRecord(RACE)/DATA and UESP RACE.
//! The record remains authoritative. This view supplies initial actor values
//! and unarmed inputs, not an independent race database.

use soe_esm::{Error, Record, Tag};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    pub health: f32,
    pub magicka: f32,
    pub stamina: f32,
    pub carry_weight: f32,
    pub mass: f32,
    pub heal_rate: f32,
    pub magicka_rate: f32,
    pub stamina_rate: f32,
    pub unarmed_damage: f32,
    pub unarmed_reach: f32,
    /// Height multiplier, male and female, `DATA` offsets 16 and 20. Used
    /// with an NPC_'s own `NAM6` the same way `TESNPC::GetHeight()` does,
    /// `race->data.height[sex] * npc.height`, for a runtime-generated actor
    /// controller capsule when the skeleton has no authored one.
    pub height_male: f32,
    pub height_female: f32,
    /// Weight multiplier, male and female, `DATA` offsets 24 and 28.
    pub weight_male: f32,
    pub weight_female: f32,
}

impl Attributes {
    /// Read the mandatory DATA prefix. Absence remains distinguishable from zero.
    ///
    /// # Errors
    /// Rejects a wrong record type, short DATA or non-finite attribute.
    pub fn from_record(record: &Record) -> Result<Option<Self>, Error> {
        if record.tag != Tag(*b"RACE") {
            return Err(Error::Invalid("race attributes require RACE".into()));
        }
        let Some(data) = record.find(Tag(*b"DATA")) else {
            return Ok(None);
        };
        if data.len() < 124 {
            return Err(Error::Invalid("short RACE DATA".into()));
        }
        let field = |offset| -> Result<f32, Error> {
            let bytes = data
                .get(offset..offset + 4)
                .ok_or_else(|| Error::Invalid("short RACE attribute".into()))?;
            let value = f32::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::Invalid("short RACE attribute".into()))?,
            );
            if !value.is_finite() {
                return Err(Error::Invalid("non-finite RACE attribute".into()));
            }
            Ok(value)
        };
        Ok(Some(Self {
            health: field(36)?,
            magicka: field(40)?,
            stamina: field(44)?,
            carry_weight: field(48)?,
            mass: field(52)?,
            heal_rate: field(84)?,
            magicka_rate: field(88)?,
            stamina_rate: field(92)?,
            unarmed_damage: field(96)?,
            unarmed_reach: field(100)?,
            height_male: field(16)?,
            height_female: field(20)?,
            weight_male: field(24)?,
            weight_female: field(28)?,
        }))
    }
}
