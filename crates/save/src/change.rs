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

    /// Decode the source-defined change flags and preserve the subtype payload
    /// for PFLA, PHZD or PBAR.
    ///
    /// The available save definitions do not establish subtype payload fields.
    /// The bytes therefore remain opaque while the shared change flag word is
    /// exposed as typed flags.
    ///
    /// # Errors
    /// Returns an error if an internally compressed payload is malformed.
    pub fn projectile_change(&self) -> Result<Option<ProjectileChange>> {
        if !matches!(self.record_type(), "PFLA" | "PHZD" | "PBAR") {
            return Ok(None);
        }
        Ok(Some(ProjectileChange {
            flags: ProjectileChangeFlags(self.change_flags),
            payload: self.payload()?.into_owned(),
        }))
    }

    /// Decode the source-defined flags and retain the opaque payload for
    /// PMIS, PGRE or PBEA change forms.
    ///
    /// xEdit defines the common 32-bit change flag word for these types, but
    /// its subtype payload branches are empty. The payload remains raw.
    ///
    /// # Errors
    /// Returns an error if an internally compressed payload is malformed.
    pub fn saved_projectile_change(&self) -> Result<Option<ProjectileChange>> {
        if !matches!(self.record_type(), "PMIS" | "PGRE" | "PBEA") {
            return Ok(None);
        }
        Ok(Some(ProjectileChange {
            flags: ProjectileChangeFlags(self.change_flags),
            payload: self.payload()?.into_owned(),
        }))
    }
    /// Return the known PCON change-form envelope without interpreting its
    /// source-opaque payload.
    pub fn pcon_change(&self) -> Option<PconChange> {
        (self.record_type() == "PCON").then(|| PconChange {
            change_flags: self.change_flags,
            version: self.version,
            uncompressed_len: self.uncompressed_len,
            data: self.data.clone(),
        })
    }

    /// Return the known PARW change-form envelope without interpreting its
    /// source-opaque payload.
    pub fn parw_change(&self) -> Option<ParwChange> {
        (self.record_type() == "PARW").then(|| ParwChange {
            change_flags: self.change_flags,
            version: self.version,
            uncompressed_len: self.uncompressed_len,
            data: self.data.clone(),
        })
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

    /// Decode the source-defined BOOK saved fields.
    ///
    /// # Errors
    /// Rejects reserved flags and truncated form flags.
    pub fn book_change(&self) -> Result<Option<BookChange>> {
        if self.record_type() != "BOOK" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "BOOK change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        Ok(Some(BookChange {
            form_flags,
            value_changed: self.change_flags & book_note_key_flags::VALUE != 0,
            full_name_changed: self.change_flags & book_note_key_flags::FULL_NAME != 0,
            teaches_skill: self.change_flags & book_note_key_flags::BOOK_TEACHES != 0,
            read: self.change_flags & book_note_key_flags::BOOK_READ != 0,
            unnamed_flags: self.change_flags & !book_note_key_flags::BOOK_KNOWN,
            opaque_suffix: cursor.rest().to_vec(),
        }))
    }

    /// Decode the source-defined KEYM saved fields.
    ///
    /// # Errors
    /// Rejects reserved flags and truncated form flags.
    pub fn key_change(&self) -> Result<Option<KeyChange>> {
        if self.record_type() != "KEYM" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "KEYM change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        Ok(Some(KeyChange {
            form_flags,
            value_changed: self.change_flags & book_note_key_flags::VALUE != 0,
            full_name_changed: self.change_flags & book_note_key_flags::FULL_NAME != 0,
            unnamed_flags: self.change_flags & !book_note_key_flags::KEY_KNOWN,
            opaque_suffix: cursor.rest().to_vec(),
        }))
    }

    /// Decode the source-defined NOTE saved fields.
    ///
    /// # Errors
    /// Rejects reserved flags and truncated form flags.
    pub fn note_change(&self) -> Result<Option<NoteChange>> {
        if self.record_type() != "NOTE" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "NOTE change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        Ok(Some(NoteChange {
            form_flags,
            read: self.change_flags & book_note_key_flags::NOTE_READ != 0,
            unnamed_flags: self.change_flags & !book_note_key_flags::NOTE_KNOWN,
            opaque_suffix: cursor.rest().to_vec(),
        }))
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

    /// Decode ACTI, TACT and DOOR base-object change fields.
    ///
    /// xEdit defines zero-byte markers for value, full-name and TACT speaker
    /// changes. The optional form flags are the only payload bytes these
    /// records define.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn activator_change(&self) -> Result<Option<ActivatorChange>> {
        let source_flags = match self.record_type() {
            "ACTI" | "DOOR" => 0b111,
            "TACT" => 0x0080_0007,
            _ => return Ok(None),
        };
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "activator change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(ActivatorChange {
            form_flags,
            value_changed: self.change_flags & 2 != 0,
            full_name_changed: self.change_flags & 4 != 0,
            speaker_changed: self.record_type() == "TACT" && self.change_flags & (1 << 23) != 0,
            other_flags: self.change_flags & !source_flags,
        }))
    }

    /// Decode INGR form flags and the ingredient-use change marker.
    ///
    /// # Errors
    /// Rejects truncated form flags and payload bytes outside xEdit's layout.
    pub fn ingredient_change(&self) -> Result<Option<IngredientChange>> {
        if self.record_type() != "INGR" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "INGR change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(IngredientChange {
            form_flags,
            ingredient_use_changed: self.change_flags & (1 << 31) != 0,
            other_flags: self.change_flags & 0x7FFF_FFFE,
        }))
    }

    /// Decode ALCH form flags and base-object change markers.
    ///
    /// # Errors
    /// Rejects truncated form flags and payload bytes outside xEdit's layout.
    pub fn alchemy_change(&self) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != "ALCH" {
            return Ok(None);
        }
        self.base_object_change_fields("ALCH change").map(Some)
    }

    /// Decode ENCH form flags and base-object change markers.
    ///
    /// # Errors
    /// Rejects truncated form flags and payload bytes outside xEdit's layout.
    pub fn enchantment_change(&self) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != "ENCH" {
            return Ok(None);
        }
        self.base_object_change_fields("ENCH change").map(Some)
    }

    fn base_object_change_fields(&self, region: &'static str) -> Result<BaseObjectChange> {
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, region);
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(BaseObjectChange {
            form_flags,
            value_changed: self.change_flags & 2 != 0,
            fullname_changed: self.change_flags & 4 != 0,
            other_flags: self.change_flags & !7,
            base_object_value_changed: false,
            base_object_fullname_changed: false,
            unnamed_flags: 0,
        })
    }
    /// Decode the saved package flags and their zero-byte markers.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn pack_change(&self) -> Result<Option<PackChange>> {
        if self.record_type() != "PACK" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "PACK change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(PackChange {
            form_flags,
            waiting: self.change_flags & (1 << 30) != 0,
            never_run: self.change_flags & (1 << 31) != 0,
            unnamed_flags: self.change_flags & 0x3FFF_FFFE,
        }))
    }

    /// Decode the saved navigation-mesh base-object fields.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn navmesh_change(&self) -> Result<Option<NavmeshChange>> {
        if self.record_type() != "NAVM" {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "NAVM change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(NavmeshChange {
            form_flags,
            value_changed: self.change_flags & 2 != 0,
            full_name_changed: self.change_flags & 4 != 0,
            unnamed_flags: self.change_flags & 0xFFFF_FFF8,
        }))
    }
    /// Decode the source-defined WOOP payload sections.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn word_of_power_change(&self) -> Result<Option<BaseObjectChange>> {
        self.word_magic_change("WOOP")
    }

    /// Decode the source-defined MGEF payload sections.
    ///
    /// # Errors
    /// Rejects truncated form flags and trailing payload bytes.
    pub fn magic_effect_change(&self) -> Result<Option<BaseObjectChange>> {
        self.word_magic_change("MGEF")
    }

    fn word_magic_change(&self, expected: &'static str) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != expected {
            return Ok(None);
        }
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "base object change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        finish_record(&cursor)?;
        Ok(Some(BaseObjectChange {
            form_flags,
            value_changed: self.change_flags & (1 << 1) != 0,
            fullname_changed: self.change_flags & (1 << 2) != 0,
            other_flags: self.change_flags & !0b111,
            base_object_value_changed: self.change_flags & (1 << 1) != 0,
            base_object_fullname_changed: self.change_flags & (1 << 2) != 0,
            unnamed_flags: self.change_flags & !0b111,
        }))
    }
    /// Decode the CELL fields whose serialized layouts xEdit defines.
    ///
    /// Seen data precedes the name and owner. Its shape is selected by matching
    /// a fixed exterior block or a sequence of interior entries against those
    /// following fields. CELL flags 28 to 30 have no serialized layout and are
    /// rejected so their bytes are not assigned an inferred meaning.
    ///
    /// # Errors
    /// Rejects unsupported flags, truncated fields, invalid seen-data lengths
    /// and trailing bytes.
    pub fn cell_change(&self) -> Result<Option<CellChange>> {
        if self.record_type() != "CELL" {
            return Ok(None);
        }
        let supported = 1 | 2 | 4 | 8 | (1 << 31);
        check_spatial_flags(self.change_flags, supported)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "CELL change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        let cell_flags = if self.change_flags & 2 != 0 {
            Some(cursor.u16()?)
        } else {
            None
        };
        let remainder = cursor.rest();
        let (seen_data, suffix_offset) = if self.change_flags & (1 << 31) != 0 {
            let mut matches = Vec::new();
            for seen_len in std::iter::once(32).chain((0..=254).map(|count| count * 34)) {
                if seen_len <= remainder.len()
                    && parse_cell_suffix(&remainder[seen_len..], self.change_flags).is_ok()
                {
                    matches.push(seen_len);
                }
            }
            if matches.len() != 1 {
                return Err(invalid_spatial_change(
                    "CELL seen-data boundary is invalid or ambiguous",
                ));
            }
            let seen_len = matches[0];
            let seen = if seen_len == 32 {
                CellSeenData::Exterior(remainder[..seen_len].to_vec())
            } else {
                let mut seen_cursor = Cursor::new(&remainder[..seen_len], "CELL seen data");
                let mut entries = Vec::with_capacity(seen_len / 34);
                while !seen_cursor.is_empty() {
                    entries.push(CellInteriorSeenEntry {
                        first: seen_cursor.u8()?,
                        second: seen_cursor.u8()? as i8,
                        data: seen_cursor.take(32)?.to_vec(),
                    });
                }
                CellSeenData::Interior(entries)
            };
            (Some(seen), seen_len)
        } else {
            (None, 0)
        };
        let (full_name, owner) = parse_cell_suffix(&remainder[suffix_offset..], self.change_flags)?;
        Ok(Some(CellChange {
            form_flags,
            cell_flags,
            full_name,
            owner,
            seen_data,
        }))
    }

    /// Decode the defined common flags of an ECZN change and retain its
    /// source-undefined flagged payload bytes verbatim.
    ///
    /// # Errors
    /// Rejects unknown flags and truncated common form flags.
    pub fn encounter_zone_change(&self) -> Result<Option<OpaqueSpatialChange>> {
        self.opaque_spatial_change("ECZN", 0x8000_0003)
    }

    /// Decode the defined common flags of an LCTN change and retain its
    /// source-undefined flagged payload bytes verbatim.
    ///
    /// # Errors
    /// Rejects unknown flags and truncated common form flags.
    pub fn location_change(&self) -> Result<Option<OpaqueSpatialChange>> {
        self.opaque_spatial_change("LCTN", 0xC000_0001)
    }

    fn opaque_spatial_change(
        &self,
        record_type: &'static str,
        supported_flags: u32,
    ) -> Result<Option<OpaqueSpatialChange>> {
        if self.record_type() != record_type {
            return Ok(None);
        }
        check_spatial_flags(self.change_flags, supported_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "spatial change");
        let form_flags = record_form_flags(&mut cursor, self.change_flags)?;
        Ok(Some(OpaqueSpatialChange {
            form_flags,
            change_flags: self.change_flags,
            opaque_payload: cursor.rest().to_vec(),
        }))
    }
}

/// Change flags for BOOK, KEYM and NOTE saved forms.
pub mod book_note_key_flags {
    /// Presence of the six-byte form-flags section.
    pub const FORM: u32 = 1;
    /// xEdit marks this base-object field changed without payload bytes.
    pub const VALUE: u32 = 1 << 1;
    /// xEdit marks this base-object field changed without payload bytes.
    pub const FULL_NAME: u32 = 1 << 2;
    /// BOOK teaches a skill, with no payload bytes.
    pub const BOOK_TEACHES: u32 = 1 << 5;
    /// BOOK has been read, with no payload bytes.
    pub const BOOK_READ: u32 = 1 << 6;
    /// NOTE has been read, with no payload bytes.
    pub const NOTE_READ: u32 = 1 << 31;
    /// Flags represented by the BOOK change decoder.
    pub const BOOK_KNOWN: u32 = FORM | VALUE | FULL_NAME | BOOK_TEACHES | BOOK_READ;
    /// Flags represented by the KEYM change decoder.
    pub const KEY_KNOWN: u32 = FORM | VALUE | FULL_NAME;
    /// Flags represented by the NOTE change decoder.
    pub const NOTE_KNOWN: u32 = FORM | NOTE_READ;
}

/// Typed BOOK saved fields. The named markers consume no payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookChange {
    pub form_flags: Option<FormFlags>,
    pub value_changed: bool,
    pub full_name_changed: bool,
    pub teaches_skill: bool,
    pub read: bool,
    /// Change flag bits xEdit names as unnamed, retained without interpretation.
    pub unnamed_flags: u32,
    /// Bytes after the xEdit-defined fields, retained without interpretation.
    pub opaque_suffix: Vec<u8>,
}

impl BookChange {
    /// Rebuild the source-defined change flag word.
    pub fn change_flags(&self) -> u32 {
        optional_flag(self.form_flags.is_some(), book_note_key_flags::FORM)
            | optional_flag(self.value_changed, book_note_key_flags::VALUE)
            | optional_flag(self.full_name_changed, book_note_key_flags::FULL_NAME)
            | optional_flag(self.teaches_skill, book_note_key_flags::BOOK_TEACHES)
            | optional_flag(self.read, book_note_key_flags::BOOK_READ)
            | self.unnamed_flags
    }

    /// Rebuild the BOOK payload.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out.extend_from_slice(&self.opaque_suffix);
        out
    }
}

/// Typed KEYM saved fields. The named base-object markers consume no bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyChange {
    pub form_flags: Option<FormFlags>,
    pub value_changed: bool,
    pub full_name_changed: bool,
    /// Change flag bits xEdit names as unnamed, retained without interpretation.
    pub unnamed_flags: u32,
    /// Bytes after the xEdit-defined fields, retained without interpretation.
    pub opaque_suffix: Vec<u8>,
}

impl KeyChange {
    /// Rebuild the source-defined change flag word.
    pub fn change_flags(&self) -> u32 {
        optional_flag(self.form_flags.is_some(), book_note_key_flags::FORM)
            | optional_flag(self.value_changed, book_note_key_flags::VALUE)
            | optional_flag(self.full_name_changed, book_note_key_flags::FULL_NAME)
            | self.unnamed_flags
    }

    /// Rebuild the KEYM payload.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out.extend_from_slice(&self.opaque_suffix);
        out
    }
}

/// Typed NOTE saved fields. The read marker consumes no payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteChange {
    pub form_flags: Option<FormFlags>,
    pub read: bool,
    /// Change flag bits xEdit names as unnamed, retained without interpretation.
    pub unnamed_flags: u32,
    /// Bytes after the xEdit-defined fields, retained without interpretation.
    pub opaque_suffix: Vec<u8>,
}

impl NoteChange {
    /// Rebuild the source-defined change flag word.
    pub fn change_flags(&self) -> u32 {
        optional_flag(self.form_flags.is_some(), book_note_key_flags::FORM)
            | optional_flag(self.read, book_note_key_flags::NOTE_READ)
            | self.unnamed_flags
    }

    /// Rebuild the NOTE payload.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out.extend_from_slice(&self.opaque_suffix);
        out
    }
}

fn optional_flag(present: bool, flag: u32) -> u32 {
    if present {
        flag
    } else {
        0
    }
}

/// Saved ACTI, TACT and DOOR change markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivatorChange {
    /// The optional six-byte form flags section.
    pub form_flags: Option<FormFlags>,
    /// Whether the source marks the zero-byte base-object value field changed.
    pub value_changed: bool,
    /// Whether the source marks the zero-byte full-name field changed.
    pub full_name_changed: bool,
    /// Whether the TACT zero-byte speaker field is marked changed.
    pub speaker_changed: bool,
    /// Bits xEdit labels unnamed, retained without assigning meaning.
    pub other_flags: u32,
}

impl ActivatorChange {
    /// Rebuild the record's change flag word.
    pub fn change_flags(self, record_type: &str) -> Result<u32> {
        if !matches!(record_type, "ACTI" | "TACT" | "DOOR") {
            return Err(invalid_record("unsupported activator change type"));
        }
        let source_flags = if record_type == "TACT" {
            0x0080_0007
        } else {
            0b111
        };
        if self.other_flags & source_flags != 0 {
            return Err(invalid_record(
                "unnamed flags overlap source-defined activator markers",
            ));
        }
        let speaker = if self.speaker_changed {
            if record_type != "TACT" {
                return Err(invalid_record("speaker marker belongs only to TACT"));
            }
            1 << 23
        } else {
            0
        };
        Ok(u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed) << 1)
            | (u32::from(self.full_name_changed) << 2)
            | speaker
            | self.other_flags)
    }

    /// Rebuild the source-defined payload bytes.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// Saved INGR sections. The ingredient-use marker has no payload in xEdit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngredientChange {
    pub form_flags: Option<FormFlags>,
    pub ingredient_use_changed: bool,
    /// Bits that xEdit labels unnamed, retained without assigning meaning.
    pub other_flags: u32,
}

impl IngredientChange {
    /// Rebuild all source-defined and unnamed change bits.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.ingredient_use_changed) << 31)
            | self.other_flags
    }

    /// Rebuild the typed payload.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// Saved base-object sections. Marker bits carry no payload in xEdit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseObjectChange {
    pub form_flags: Option<FormFlags>,
    /// Value marker, also exposed under the legacy WOOP/MGEF field name below.
    pub value_changed: bool,
    /// Full-name marker, also exposed under the legacy WOOP/MGEF field name below.
    pub fullname_changed: bool,
    /// Bits that xEdit labels unnamed, retained without assigning meaning.
    pub other_flags: u32,
    /// WOOP/MGEF alias for `value_changed`.
    pub base_object_value_changed: bool,
    /// WOOP/MGEF alias for `fullname_changed`.
    pub base_object_fullname_changed: bool,
    /// WOOP/MGEF alias for `other_flags`.
    pub unnamed_flags: u32,
}

impl BaseObjectChange {
    /// Rebuild all source-defined and unnamed change bits.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed || self.base_object_value_changed) << 1)
            | (u32::from(self.fullname_changed || self.base_object_fullname_changed) << 2)
            | self.other_flags
            | self.unnamed_flags
    }

    /// Rebuild the typed payload.
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out
    }
}

/// Defined CELL change fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellChange {
    pub form_flags: Option<FormFlags>,
    pub cell_flags: Option<u16>,
    pub full_name: Option<crate::cursor::WString>,
    pub owner: Option<RefId>,
    pub seen_data: Option<CellSeenData>,
}

/// CELL seen data in the fixed exterior or repeated interior shape from xEdit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellSeenData {
    Exterior(Vec<u8>),
    Interior(Vec<CellInteriorSeenEntry>),
}

/// One interior CELL seen-data entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellInteriorSeenEntry {
    pub first: u8,
    pub second: i8,
    pub data: Vec<u8>,
}

impl CellChange {
    /// Rebuild the decoded fields in their serialized order.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        if let Some(flags) = self.cell_flags {
            out.extend(flags.to_le_bytes());
        }
        if let Some(seen_data) = &self.seen_data {
            match seen_data {
                CellSeenData::Exterior(bytes) => out.extend(bytes),
                CellSeenData::Interior(entries) => {
                    for entry in entries {
                        out.push(entry.first);
                        out.push(entry.second as u8);
                        out.extend(&entry.data);
                    }
                }
            }
        }
        if let Some(name) = &self.full_name {
            name.write(&mut out);
        }
        if let Some(owner) = self.owner {
            owner.write(&mut out);
        }
        out
    }
}

/// Common form flags and untouched payload bytes for spatial records whose
/// flagged field layouts are placeholders in the available save schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueSpatialChange {
    pub form_flags: Option<FormFlags>,
    pub change_flags: u32,
    pub opaque_payload: Vec<u8>,
}

fn check_spatial_flags(flags: u32, known: u32) -> Result<()> {
    if flags & !known != 0 {
        return Err(invalid_spatial_change("unknown change flags"));
    }
    Ok(())
}

fn invalid_spatial_change(reason: &str) -> Error {
    Error::SpatialChange {
        reason: reason.into(),
    }
}

fn parse_cell_suffix(
    bytes: &[u8],
    change_flags: u32,
) -> Result<(Option<crate::cursor::WString>, Option<RefId>)> {
    let mut cursor = Cursor::new(bytes, "CELL change");
    let full_name = if change_flags & 4 != 0 {
        Some(cursor.wstring()?)
    } else {
        None
    };
    let owner = if change_flags & 8 != 0 {
        Some(cursor.ref_id()?)
    } else {
        None
    };
    finish_record(&cursor)?;
    Ok((full_name, owner))
}

impl OpaqueSpatialChange {
    /// Rebuild the common form flags followed by the retained opaque bytes.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_record_form_flags(self.form_flags, &mut out);
        out.extend(&self.opaque_payload);
        out
    }
}

/// One source-defined PFLA, PHZD or PBAR change flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProjectileChangeFlag {
    /// Record form flags changed.
    FormFlags = 0,
    /// The reference moved.
    Move = 1,
    /// Havok movement changed.
    HavokMove = 2,
    /// The reference changed cells.
    CellChanged = 3,
    /// The reference scale changed.
    Scale = 4,
    /// Inventory changed.
    Inventory = 5,
    /// Ownership extra data changed.
    ExtraOwnership = 6,
    /// The base object changed.
    BaseObject = 7,
    /// Source flag without an assigned name.
    Unnamed08 = 8,
    /// Source flag without an assigned name.
    Unnamed09 = 9,
    /// Item data extra changed.
    ExtraItemData = 10,
    /// Ammo extra changed.
    ExtraAmmo = 11,
    /// Lock extra changed.
    ExtraLock = 12,
    /// Source flag without an assigned name.
    Unnamed13 = 13,
    /// Source flag without an assigned name.
    Unnamed14 = 14,
    /// Source flag without an assigned name.
    Unnamed15 = 15,
    /// Source flag without an assigned name.
    Unnamed16 = 16,
    /// Source flag without an assigned name.
    Unnamed17 = 17,
    /// Source flag without an assigned name.
    Unnamed18 = 18,
    /// Source flag without an assigned name.
    Unnamed19 = 19,
    /// Source flag without an assigned name.
    Unnamed20 = 20,
    /// Object empty state changed.
    Empty = 21,
    /// Object default open state changed.
    OpenDefaultState = 22,
    /// Object open state changed.
    OpenState = 23,
    /// Source flag without an assigned name.
    Unnamed24 = 24,
    /// The reference was promoted.
    Promoted = 25,
    /// Activating children extra data changed.
    ExtraActivatingChildren = 26,
    /// Leveled inventory changed.
    LeveledInventory = 27,
    /// Animation changed.
    Animation = 28,
    /// Encounter zone extra data changed.
    ExtraEncounterZone = 29,
    /// Created-only extra data changed.
    ExtraCreatedOnly = 30,
    /// Game-only extra data changed.
    ExtraGameOnly = 31,
}

impl ProjectileChangeFlag {
    /// All bit positions defined by xEdit for these change types.
    pub const ALL: [Self; 32] = [
        Self::FormFlags,
        Self::Move,
        Self::HavokMove,
        Self::CellChanged,
        Self::Scale,
        Self::Inventory,
        Self::ExtraOwnership,
        Self::BaseObject,
        Self::Unnamed08,
        Self::Unnamed09,
        Self::ExtraItemData,
        Self::ExtraAmmo,
        Self::ExtraLock,
        Self::Unnamed13,
        Self::Unnamed14,
        Self::Unnamed15,
        Self::Unnamed16,
        Self::Unnamed17,
        Self::Unnamed18,
        Self::Unnamed19,
        Self::Unnamed20,
        Self::Empty,
        Self::OpenDefaultState,
        Self::OpenState,
        Self::Unnamed24,
        Self::Promoted,
        Self::ExtraActivatingChildren,
        Self::LeveledInventory,
        Self::Animation,
        Self::ExtraEncounterZone,
        Self::ExtraCreatedOnly,
        Self::ExtraGameOnly,
    ];

    const fn mask(self) -> u32 {
        1_u32 << self as u8
    }
}

/// The shared xEdit change flag word used by PFLA, PHZD and PBAR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectileChangeFlags(u32);

impl ProjectileChangeFlags {
    /// Return the original flag word, including unnamed source bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Check whether a source-defined flag is set.
    #[must_use]
    pub const fn contains(self, flag: ProjectileChangeFlag) -> bool {
        self.0 & flag.mask() != 0
    }
}

/// Typed change flags and the preserved payload for a projectile or hazard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectileChange {
    /// The shared, source-defined change flags.
    pub flags: ProjectileChangeFlags,
    /// The exact payload bytes, since no inspected source defines subtype fields.
    pub payload: Vec<u8>,
}

/// PCON change-form data as defined by the known Skyrim save schema.
///
/// The schema defines no subtype fields, so `data` retains its original bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PconChange {
    pub change_flags: u32,
    pub version: u8,
    pub uncompressed_len: u32,
    pub data: Vec<u8>,
}

/// PARW change-form data as defined by the known Skyrim save schema.
///
/// The schema defines no subtype fields, so `data` retains its original bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParwChange {
    pub change_flags: u32,
    pub version: u8,
    pub uncompressed_len: u32,
    pub data: Vec<u8>,
}

#[cfg(test)]
mod sav_55_projectile_envelope_tests {
    use super::ChangeForm;
    use crate::{cursor::Cursor, cursor::RefId};

    #[test]
    fn sav_55_projectile_subtype_ids_preserve_flags_and_payload_envelope() {
        let subtypes = [
            (2, "PMIS"),
            (3, "PGRE"),
            (4, "PBEA"),
            (5, "PFLA"),
            (40, "PHZD"),
            (41, "PBAR"),
            (42, "PCON"),
            (47, "PARW"),
        ];
        let payload = [0x10, 0x20, 0x30, 0x40, 0x50];

        for (index, expected_type) in subtypes {
            let flags = 1_u32 << (index % 32);
            let form = ChangeForm {
                form: RefId::from_bytes([0; 3]),
                change_flags: flags,
                type_field: index,
                version: 0,
                uncompressed_len: 0,
                data: payload.to_vec(),
            };
            let mut bytes = Vec::new();
            form.write(&mut bytes);
            let mut cursor = Cursor::new(&bytes, "SAV-55 projectile fixture");
            let decoded = ChangeForm::read(&mut cursor).unwrap();

            assert_eq!(decoded.record_type(), expected_type);
            assert_eq!(decoded.change_flags, flags);
            assert_eq!(decoded.data, payload);
            assert_eq!(cursor.remaining(), 0);
        }
    }
}

#[cfg(test)]
mod sav_23_projectile_tests {
    use super::{ChangeForm, ProjectileChangeFlag};
    use crate::{cursor::Cursor, cursor::RefId};

    #[test]
    fn sav_23_projectile_flags_and_payload_bytes_survive_all_types() {
        for type_index in [5_u8, 40, 41] {
            for flag in ProjectileChangeFlag::ALL {
                let bits = 1_u32 << flag as u8;
                let change = ChangeForm {
                    form: RefId::from_bytes([0; 3]),
                    change_flags: bits,
                    type_field: type_index,
                    version: 0,
                    uncompressed_len: 0,
                    data: vec![0x00, 0x81, 0xFF, flag as u8],
                };
                let decoded = change
                    .projectile_change()
                    .unwrap()
                    .expect("projectile type must decode");
                assert_eq!(decoded.flags.bits(), bits);
                assert!(decoded.flags.contains(flag));
                assert_eq!(decoded.payload, change.data);
            }

            let change = ChangeForm {
                form: RefId::from_bytes([0; 3]),
                change_flags: u32::MAX,
                type_field: type_index,
                version: 0,
                uncompressed_len: 0,
                data: vec![0xFE, 0x00, 0x7F],
            };
            let decoded = change.projectile_change().unwrap().unwrap();
            assert_eq!(decoded.flags.bits(), u32::MAX);
            assert_eq!(decoded.payload, change.data);
        }
    }

    #[test]
    fn sav_23_non_projectile_change_returns_none() {
        let change = ChangeForm {
            form: RefId::from_bytes([0; 3]),
            change_flags: 0,
            type_field: 2,
            version: 0,
            uncompressed_len: 0,
            data: vec![],
        };
        assert!(change.projectile_change().unwrap().is_none());
    }

    #[test]
    fn sav_23_truncated_projectile_change_envelope_is_rejected() {
        let mut bytes = vec![0, 0, 0];
        bytes.extend_from_slice(&0x1234_u32.to_le_bytes());
        bytes.extend_from_slice(&[5, 0, 4, 0, 1, 2]);
        let error = ChangeForm::read(&mut Cursor::new(&bytes, "change form")).unwrap_err();
        assert!(matches!(error, crate::Error::Truncated { .. }));
    }
}

#[cfg(test)]
mod sav_25_projectile_truncation_tests {
    use super::ChangeForm;
    use crate::{cursor::Cursor, cursor::RefId};

    #[test]
    fn sav_25_change_form_reader_rejects_each_truncated_envelope_field() {
        for kind in [2, 3, 4, 5, 40, 41, 42, 47] {
            let form = ChangeForm {
                form: RefId::from_bytes([0; 3]),
                change_flags: u32::MAX,
                type_field: kind,
                version: 74,
                uncompressed_len: 0,
                data: vec![0x10, 0x20, 0x30],
            };
            let mut bytes = Vec::new();
            form.write(&mut bytes);

            for end in 0..bytes.len() {
                let mut cursor = Cursor::new(&bytes[..end], "truncated change form");
                assert!(
                    ChangeForm::read(&mut cursor).is_err(),
                    "kind {kind}, end {end}"
                );
            }
        }
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

/// Compatibility name for the WOOP/MGEF view of [`BaseObjectChange`].
pub type BaseObjectMarkerChange = BaseObjectChange;

/// Saved PACK fields. Waiting and never-run are zero-byte change markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackChange {
    pub form_flags: Option<FormFlags>,
    pub waiting: bool,
    pub never_run: bool,
    /// xEdit-defined flag bits with no assigned meaning, bits 1 through 29.
    pub unnamed_flags: u32,
}

impl PackChange {
    /// Rebuild the change flag word from the present sections.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.waiting) << 30)
            | (u32::from(self.never_run) << 31)
            | self.unnamed_flags
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
    /// xEdit-defined flag bits with no assigned meaning, bits 3 through 31.
    pub unnamed_flags: u32,
}

impl NavmeshChange {
    /// Rebuild the change flag word from the present sections.
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed) << 1)
            | (u32::from(self.full_name_changed) << 2)
            | self.unnamed_flags
    }

    /// Rebuild the source-defined NAVM payload.
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
