// SPDX-License-Identifier: GPL-3.0-or-later

//! Change forms: the per object record of everything play has altered since
//! the plugins were loaded.
//!
//! A change form is a header plus an opaque payload whose shape depends on the
//! form's record type and on which bits of `change_flags` are set. Only the
//! header is decoded for every form. The payload is kept as raw bytes and
//! decoded per type on demand, which is how a save with a type this crate does
//! not know still round trips byte for byte.
//!
//! The payload has its own compression, separate from the file's: when
//! `length2` is non-zero the payload is zlib deflated and `length2` is its
//! inflated size.

use crate::cursor::{Cursor, RefId, VsVal};
use crate::error::{Error, Result};

/// The record type a change form index names, in the order the type byte's
/// low six bits use. Taken from the UESP Save File Format page and matched
/// against ReSaver's `resaver.ess.Type`.
const TYPES: [&str; 49] = [
    "REFR", "ACHR", "PMIS", "PGRE", "PBEA", "PFLA", "CELL", "INFO", "QUST", "NPC_", "ACTI", "TACT",
    "ARMO", "BOOK", "CONT", "DOOR", "INGR", "LIGH", "MISC", "APPA", "STAT", "MSTT", "FURN", "WEAP",
    "AMMO", "KEYM", "ALCH", "IDLM", "NOTE", "ECZN", "CLAS", "FACT", "PACK", "NAVM", "WOOP", "MGEF",
    "SMQN", "SCEN", "LCTN", "RELA", "PHZD", "PBAR", "PCON", "FLST", "LVLN", "LVLI", "LVSP", "PARW",
    "ENCH",
];

/// Change flag bits on a `QUST` change form, from ReSaver's
/// `ChangeFlagConstantsQust`.
mod quest_flags {
    pub const CHANGE_FORM_FLAGS: u32 = 1 << 0;
    pub const QUEST_FLAGS: u32 = 1 << 1;
    pub const SCRIPT_DELAY: u32 = 1 << 2;
    pub const STAGES: u32 = 1 << 31;
}

#[derive(Clone, Debug)]
pub struct ChangeForm {
    pub form: RefId,
    pub change_flags: u32,
    /// The raw type byte. The low six bits index [`TYPES`], the top two give
    /// the width the two length fields were written at.
    pub type_field: u8,
    pub version: u8,
    /// The inflated length when the payload is compressed, else 0.
    pub uncompressed_len: u32,
    pub data: Vec<u8>,
}

impl ChangeForm {
    pub fn type_index(&self) -> u8 {
        self.type_field & 0x3F
    }

    /// The record type this form belongs to, for example `QUST`.
    pub fn record_type(&self) -> &'static str {
        TYPES
            .get(self.type_index() as usize)
            .copied()
            .unwrap_or("????")
    }

    pub fn is_compressed(&self) -> bool {
        self.uncompressed_len > 0
    }

    fn length_width(&self) -> usize {
        match self.type_field >> 6 {
            0 => 1,
            1 => 2,
            _ => 4,
        }
    }

    pub fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let at = cursor.pos();
        let form = cursor.ref_id()?;
        let change_flags = cursor.u32()?;
        let type_field = cursor.u8()?;
        let version = cursor.u8()?;

        let index = type_field & 0x3F;
        if index as usize >= TYPES.len() {
            return Err(Error::ChangeFormType { at, index });
        }

        let (length1, uncompressed_len) = match type_field >> 6 {
            0 => (u32::from(cursor.u8()?), u32::from(cursor.u8()?)),
            1 => (u32::from(cursor.u16()?), u32::from(cursor.u16()?)),
            _ => (cursor.u32()?, cursor.u32()?),
        };

        let data = cursor.take(length1 as usize)?.to_vec();

        Ok(ChangeForm {
            form,
            change_flags,
            type_field,
            version,
            uncompressed_len,
            data,
        })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        self.form.write(out);
        out.extend_from_slice(&self.change_flags.to_le_bytes());
        out.push(self.type_field);
        out.push(self.version);
        let length1 = self.data.len() as u32;
        match self.length_width() {
            1 => {
                out.push(length1 as u8);
                out.push(self.uncompressed_len as u8);
            }
            2 => {
                out.extend_from_slice(&(length1 as u16).to_le_bytes());
                out.extend_from_slice(&(self.uncompressed_len as u16).to_le_bytes());
            }
            _ => {
                out.extend_from_slice(&length1.to_le_bytes());
                out.extend_from_slice(&self.uncompressed_len.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.data);
    }

    /// The payload, inflated when the form carries it compressed.
    pub fn payload(&self) -> Result<std::borrow::Cow<'_, [u8]>> {
        if !self.is_compressed() {
            return Ok(std::borrow::Cow::Borrowed(&self.data));
        }
        let inflated = crate::compress::inflate_zlib(&self.data, self.uncompressed_len)?;
        if inflated.len() != self.uncompressed_len as usize {
            return Err(Error::ChangeFormSize {
                form: self.form.raw(),
                expected: self.uncompressed_len,
                got: inflated.len(),
            });
        }
        Ok(std::borrow::Cow::Owned(inflated))
    }

    /// The quest stages this form records, when it is a `QUST` form whose
    /// flags say it carries them.
    ///
    /// Everything before the stages in the payload has to be stepped over
    /// first, and each of those parts is present only when its own flag bit is
    /// set, so a form missing one of them still parses.
    pub fn quest_stages(&self) -> Result<Option<Vec<QuestStage>>> {
        if self.record_type() != "QUST" || self.change_flags & quest_flags::STAGES == 0 {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "change form");

        if self.change_flags & quest_flags::CHANGE_FORM_FLAGS != 0 {
            let _flags = cursor.u32()?;
            let _unknown = cursor.u16()?;
        }
        if self.change_flags & quest_flags::QUEST_FLAGS != 0 {
            let _quest_flags = cursor.u16()?;
        }
        if self.change_flags & quest_flags::SCRIPT_DELAY != 0 {
            let _delay = cursor.f32()?;
        }

        let count = cursor.vsval()?.value();
        let mut stages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let stage = cursor.i16()?;
            let status = cursor.u8()?;
            stages.push(QuestStage { stage, status });
        }
        Ok(Some(stages))
    }
}

/// One stage entry. `status` bit 0 is the stage's done flag, which is what
/// `GetStageDone` reads in game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct QuestStage {
    pub stage: i16,
    pub status: u8,
}

impl QuestStage {
    pub fn is_done(self) -> bool {
        self.status & 0x1 != 0
    }
}

/// Writes a quest change form payload. Test support: it builds the same
/// layout [`ChangeForm::quest_stages`] reads, so a round trip can be checked
/// without a real save.
pub fn quest_payload(stages: &[QuestStage]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    VsVal::from_value(u32::try_from(stages.len()).ok()?)?.write(&mut out);
    for stage in stages {
        out.extend_from_slice(&stage.stage.to_le_bytes());
        out.push(stage.status);
    }
    Some(out)
}
