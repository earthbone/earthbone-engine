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
//! The type indices follow UESP's Save File Format page. Typed field layouts
//! use xEdit's `wbDefinitionsTES5Saves.pas` and FallrimTools change readers.

use crate::cursor::{Cursor, RefId, RefKind, VsVal};
use crate::error::{Error, Result};
use crate::quest::FormFlags;

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

    /// Decode CLAS form flags and the tag-skills change marker.
    ///
    /// # Errors
    /// Rejects unknown bits and bytes outside the defined sections.
    pub fn class_change(&self) -> Result<Option<ClassChange>> {
        if self.record_type() != "CLAS" {
            return Ok(None);
        }
        check_record_flags(self.change_flags, 0b11)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "CLAS change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(ClassChange {
            form_flags,
            tag_skills_changed: self.change_flags & 2 != 0,
        }))
    }

    /// Decode FACT flags, reactions and crime words.
    ///
    /// # Errors
    /// Rejects unknown bits, truncated fields, impossible reaction counts and
    /// trailing bytes.
    pub fn faction_change(&self) -> Result<Option<FactionChange>> {
        if self.record_type() != "FACT" {
            return Ok(None);
        }
        check_record_flags(self.change_flags, 0x8000_0007)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "FACT change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        let reactions = if self.change_flags & 4 != 0 {
            let count = cursor.vsval()?;
            if count.value() as usize > cursor.remaining() / 11 {
                return Err(invalid_record("reaction count exceeds remaining payload"));
            }
            let mut entries = Vec::with_capacity(count.value() as usize);
            for _ in 0..count.value() {
                entries.push(FactionReaction {
                    faction: cursor.ref_id()?,
                    modifier: cursor.u32()? as i32,
                    reaction: cursor.u32()?,
                });
            }
            Some(FactionReactions { count, entries })
        } else {
            None
        };
        let faction_flags = if self.change_flags & 2 != 0 {
            Some(cursor.u32()?)
        } else {
            None
        };
        let crime_counts = if self.change_flags & (1 << 31) != 0 {
            Some([cursor.u32()?, cursor.u32()?, cursor.u32()?, cursor.u32()?])
        } else {
            None
        };
        finish_record(&cursor)?;
        Ok(Some(FactionChange {
            form_flags,
            faction_flags,
            reactions,
            crime_counts,
        }))
    }

    /// Decode RELA identities for created forms and the changed rank.
    ///
    /// # Errors
    /// Rejects unknown bits, truncated fields and trailing bytes.
    pub fn relationship_change(&self) -> Result<Option<RelationshipChange>> {
        if self.record_type() != "RELA" {
            return Ok(None);
        }
        check_record_flags(self.change_flags, 0b11)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "RELA change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        let created = if self.form.kind() == RefKind::Created {
            Some([cursor.ref_id()?, cursor.ref_id()?, cursor.ref_id()?])
        } else {
            None
        };
        let rank = if self.change_flags & 2 != 0 {
            Some(cursor.u32()? as i32)
        } else {
            None
        };
        finish_record(&cursor)?;
        Ok(Some(RelationshipChange {
            form_flags,
            created,
            rank,
        }))
    }

    /// Decode the saved package flags and their zero-byte markers.
    ///
    /// # Errors
    /// Rejects unknown flag bits, truncated form flags and trailing payload
    /// bytes.
    pub fn pack_change(&self) -> Result<Option<PackChange>> {
        if self.record_type() != "PACK" {
            return Ok(None);
        }
        check_record_flags(self.change_flags, 0xC000_0001)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "PACK change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(PackChange {
            form_flags,
            waiting: self.change_flags & (1 << 30) != 0,
            never_run: self.change_flags & (1 << 31) != 0,
        }))
    }

    /// Decode the saved navigation-mesh base-object fields.
    ///
    /// # Errors
    /// Rejects unknown flag bits, truncated form flags and trailing payload
    /// bytes.
    pub fn navmesh_change(&self) -> Result<Option<NavmeshChange>> {
        if self.record_type() != "NAVM" {
            return Ok(None);
        }
        check_record_flags(self.change_flags, 0b111)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "NAVM change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(NavmeshChange {
            form_flags,
            value_changed: self.change_flags & 2 != 0,
            full_name_changed: self.change_flags & 4 != 0,
        }))
    }

    /// Decode the source-defined WOOP payload sections.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn word_of_power_change(&self) -> Result<Option<BaseObjectChange>> {
        self.base_object_change("WOOP")
    }

    /// Decode the source-defined MGEF payload sections.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn magic_effect_change(&self) -> Result<Option<BaseObjectChange>> {
        self.base_object_change("MGEF")
    }

    fn base_object_change(&self, expected: &'static str) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != expected {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "base object change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(BaseObjectChange {
            form_flags,
            base_object_value_changed: self.change_flags & (1 << 1) != 0,
            base_object_fullname_changed: self.change_flags & (1 << 2) != 0,
            unnamed_flags: self.change_flags & !0b111,
        }))
    }
}

/// Saved CLAS sections. xEdit names tag skills but defines no payload for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassChange {
    pub form_flags: Option<FormFlags>,
    pub tag_skills_changed: bool,
}

impl ClassChange {
    /// Rebuild the flag word from the present section and change marker.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some()) | (u32::from(self.tag_skills_changed) << 1)
    }

    /// Rebuild the source-defined payload section.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// A saved faction reaction with its signed modifier and raw reaction value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactionReaction {
    pub faction: RefId,
    pub modifier: i32,
    pub reaction: u32,
}

/// Reaction entries with the original variable-count encoding.
#[derive(Clone, Debug)]
pub struct FactionReactions {
    count: VsVal,
    pub entries: Vec<FactionReaction>,
}

impl FactionReactions {
    /// Make a reaction array using the narrowest count encoding.
    ///
    /// # Errors
    /// Rejects a count above the thirty-bit saved limit.
    pub fn from_entries(entries: Vec<FactionReaction>) -> Result<Self> {
        let count =
            u32::try_from(entries.len()).map_err(|_| invalid_record("too many reactions"))?;
        let count = VsVal::from_value(count).ok_or_else(|| invalid_record("too many reactions"))?;
        Ok(Self { count, entries })
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.count.value() as usize != self.entries.len() {
            return Err(invalid_record("reaction count differs from entries"));
        }
        self.count.write(out);
        for entry in &self.entries {
            entry.faction.write(out);
            out.extend(entry.modifier.to_le_bytes());
            out.extend(entry.reaction.to_le_bytes());
        }
        Ok(())
    }
}

/// Saved FACT sections. Crime words retain their bits without assigned meaning.
#[derive(Clone, Debug)]
pub struct FactionChange {
    pub form_flags: Option<FormFlags>,
    pub faction_flags: Option<u32>,
    pub reactions: Option<FactionReactions>,
    pub crime_counts: Option<[u32; 4]>,
}

impl FactionChange {
    /// Rebuild the flag word from the present sections.
    pub fn change_flags(&self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.faction_flags.is_some()) << 1)
            | (u32::from(self.reactions.is_some()) << 2)
            | (u32::from(self.crime_counts.is_some()) << 31)
    }

    /// Rebuild the typed payload.
    ///
    /// # Errors
    /// Rejects a reaction array whose retained count differs from its entries.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        if let Some(reactions) = &self.reactions {
            reactions.write(&mut out)?;
        }
        if let Some(flags) = self.faction_flags {
            out.extend(flags.to_le_bytes());
        }
        if let Some(counts) = self.crime_counts {
            for count in counts {
                out.extend(count.to_le_bytes());
            }
        }
        Ok(out)
    }
}

/// Saved RELA fields. Created forms carry three identities even without bit one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationshipChange {
    pub form_flags: Option<FormFlags>,
    pub created: Option<[RefId; 3]>,
    pub rank: Option<i32>,
}

/// Saved PACK fields. Waiting and never-run are zero-byte change markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackChange {
    pub form_flags: Option<FormFlags>,
    pub waiting: bool,
    pub never_run: bool,
}

impl PackChange {
    /// Rebuild the change flag word from the present sections.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.waiting) << 30)
            | (u32::from(self.never_run) << 31)
    }

    /// Rebuild the source-defined PACK payload.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// Saved NAVM fields. Value and full-name changes are zero-byte markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavmeshChange {
    pub form_flags: Option<FormFlags>,
    pub value_changed: bool,
    pub full_name_changed: bool,
}

impl NavmeshChange {
    /// Rebuild the change flag word from the present sections.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed) << 1)
            | (u32::from(self.full_name_changed) << 2)
    }

    /// Rebuild the source-defined NAVM payload.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// Saved WOOP or MGEF change sections. The two named base-object sections
/// carry no payload bytes in xEdit's schema. Unnamed change bits remain raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseObjectChange {
    pub form_flags: Option<FormFlags>,
    pub base_object_value_changed: bool,
    pub base_object_fullname_changed: bool,
    pub unnamed_flags: u32,
}

impl BaseObjectChange {
    /// Rebuild the complete change flag word.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.base_object_value_changed) << 1)
            | (u32::from(self.base_object_fullname_changed) << 2)
            | (self.unnamed_flags & !0b111)
    }

    /// Rebuild the source-defined payload bytes.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

impl RelationshipChange {
    /// Rebuild the flag word from the present sections.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some()) | (u32::from(self.rank.is_some()) << 1)
    }

    /// Rebuild the typed payload for the form identity supplied by the caller.
    ///
    /// # Errors
    /// Rejects a mismatch between created status and the identity section.
    pub fn data_bytes(self, form: RefId) -> Result<Vec<u8>> {
        if (form.kind() == RefKind::Created) != self.created.is_some() {
            return Err(invalid_record(
                "created identity section does not match form kind",
            ));
        }
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        if let Some(ids) = self.created {
            for id in ids {
                id.write(&mut out);
            }
        }
        if let Some(rank) = self.rank {
            out.extend(rank.to_le_bytes());
        }
        Ok(out)
    }
}

fn check_record_flags(flags: u32, known: u32) -> Result<()> {
    if flags & !known != 0 {
        return Err(invalid_record("unknown change flags"));
    }
    Ok(())
}

fn record_form_flags(cursor: &mut Cursor<'_>, flags: u32) -> Result<Option<FormFlags>> {
    if flags & 1 == 0 {
        return Ok(None);
    }
    Ok(Some(FormFlags {
        flags: cursor.u32()?,
        unknown: cursor.u16()?,
    }))
}

fn write_record_form_flags(flags: Option<FormFlags>, out: &mut Vec<u8>) {
    if let Some(flags) = flags {
        out.extend(flags.flags.to_le_bytes());
        out.extend(flags.unknown.to_le_bytes());
    }
}

fn finish_record(cursor: &Cursor<'_>) -> Result<()> {
    if !cursor.is_empty() {
        return Err(invalid_record("trailing change bytes"));
    }
    Ok(())
}

fn invalid_record(reason: &str) -> Error {
    Error::RecordChange {
        reason: reason.into(),
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
