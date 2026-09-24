// SPDX-License-Identifier: GPL-3.0-or-later

//! CLAS view from xEdit `wbRecord(CLAS)/DATA` and UESP CLAS. The 36 byte
//! layout matches CommonLibSSE-NG's `TESClass::CLASS_DATA`
//! (`include/RE/T/TESClass.h`): a 4 byte unknown prefix, `Teaches` and
//! `Maximum training level` as one byte each, 18 skill weights in the same
//! order as [`crate::npc::SKILLS`], a bleedout default float, voice points,
//! then the health, magicka and stamina attribute weights. The record
//! remains authoritative, this view only extracts the fields an
//! auto-calculated actor's skills need.
//!
//! `teaches` and `maximum_training_level` govern trainer dialogue, not level
//! up skill point distribution, and stay unconsumed here. `attribute_weights`
//! is the input to the class health, magicka and stamina weighting formula,
//! also not consumed yet, see `docs/actor-values.md`.

use soe_esm::{Error, Record, Tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Class {
    /// Trainer skill, a `Skill` index (0-17), for dialogue only.
    pub teaches: u8,
    pub maximum_training_level: u8,
    /// Level up skill weights, one per [`crate::npc::SKILLS`] entry in order.
    pub skill_weights: [u8; 18],
    /// Health, magicka, stamina weights for the class attribute formula.
    pub attribute_weights: [u8; 3],
}

impl Class {
    /// Read the mandatory DATA prefix. Absence remains distinguishable from zero.
    ///
    /// # Errors
    /// Rejects a wrong record type or short DATA.
    pub fn from_record(record: &Record) -> Result<Option<Self>, Error> {
        if record.tag != Tag(*b"CLAS") {
            return Err(Error::Invalid("class fields require CLAS".into()));
        }
        let Some(data) = record.find(Tag(*b"DATA")) else {
            return Ok(None);
        };
        if data.len() < 35 {
            return Err(Error::Invalid("short CLAS DATA".into()));
        }
        let mut skill_weights = [0u8; 18];
        skill_weights.copy_from_slice(&data[6..24]);
        Ok(Some(Self {
            teaches: data[4],
            maximum_training_level: data[5],
            skill_weights,
            attribute_weights: [data[32], data[33], data[34]],
        }))
    }
}
