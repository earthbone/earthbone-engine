// SPDX-License-Identifier: GPL-3.0-or-later

//! NPC change fields from xEdit wbChangedNPC and FallrimTools ChangeFormNPC.
//! These are record changes, not the separate actor's values or AI execution.

mod io;

use crate::cursor::{Cursor, VsVal};
use crate::{ChangeForm, Error, RefId, Result, WString};

pub mod flags {
    pub const FORM: u32 = 1;
    pub const BASE: u32 = 1 << 1;
    /// xEdit declares this flag with no serialized field.
    pub const ATTRIBUTES: u32 = 1 << 2;
    pub const AI: u32 = 1 << 3;
    pub const SPELLS: u32 = 1 << 4;
    pub const NAME: u32 = 1 << 5;
    pub const FACTIONS: u32 = 1 << 6;
    pub const SKILLS: u32 = 1 << 9;
    pub const CLASS: u32 = 1 << 10;
    pub const FACE: u32 = 1 << 11;
    pub const DEFAULT_OUTFIT: u32 = 1 << 12;
    pub const SLEEP_OUTFIT: u32 = 1 << 13;
    pub const GENDER: u32 = 1 << 24;
    pub const RACE: u32 = 1 << 25;
    pub const KNOWN: u32 = FORM
        | BASE
        | ATTRIBUTES
        | AI
        | SPELLS
        | NAME
        | FACTIONS
        | SKILLS
        | CLASS
        | FACE
        | DEFAULT_OUTFIT
        | SLEEP_OUTFIT
        | GENDER
        | RACE;
}

/// A list retains its saved count width for exact typed re-encoding.
#[derive(Debug, Clone)]
pub struct List<T> {
    pub(crate) count: VsVal,
    pub entries: Vec<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Faction {
    pub form: RefId,
    pub rank: i8,
}

#[derive(Debug, Clone)]
pub struct Spells {
    pub spells: List<RefId>,
    pub leveled_spells: List<RefId>,
    pub shouts: List<RefId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub hair_color: RefId,
    pub skin_color: [u8; 4],
    pub texture_set: RefId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Morph {
    /// Float bits preserve NaNs and negative zero without changing the save.
    pub options: Vec<u32>,
    pub presets: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct Face {
    pub(crate) head_tag: u8,
    pub head: Option<Head>,
    pub head_parts: List<RefId>,
    pub(crate) morph_tag: u8,
    pub morph: Option<Morph>,
}

#[derive(Debug, Clone)]
pub struct NpcChange {
    pub change_flags: u32,
    /// xEdit names these Mask Invert Flags and Mask Set unk010.
    pub form_flags: Option<(u32, u16)>,
    /// Same record layouts as ACBS, AIDT and DNAM in an NPC_ record.
    pub base: Option<[u8; 24]>,
    pub factions: Option<List<Faction>>,
    pub spells: Option<Spells>,
    pub ai: Option<[u8; 20]>,
    pub name: Option<WString>,
    pub skills: Option<[u8; 52]>,
    pub class: Option<RefId>,
    /// New race followed by old race, both still in the save's reference space.
    pub race: Option<(RefId, RefId)>,
    pub face: Option<Face>,
    pub gender: Option<u8>,
    pub default_outfit: Option<RefId>,
    pub sleep_outfit: Option<RefId>,
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::Npc {
        reason: reason.into(),
    }
}

impl NpcChange {
    /// Read an NPC field sequence embedded inside another change form.
    /// The returned size covers only this sequence, not the remaining buffer.
    ///
    /// # Errors
    /// Unknown flags, truncated fields or counts beyond the remaining bytes.
    pub fn read_prefix(data: &[u8], flags: u32) -> Result<(Self, usize)> {
        if flags & !flags::KNOWN != 0 {
            return Err(invalid(format!("unsupported NPC flags {flags:#x}")));
        }
        let mut r = Cursor::new(data, "NPC change");
        let value = Self {
            change_flags: flags,
            form_flags: io::optional(&mut r, flags, flags::FORM, |r| Ok((r.u32()?, r.u16()?)))?,
            base: io::optional(&mut r, flags, flags::BASE, io::bytes)?,
            factions: io::optional(&mut r, flags, flags::FACTIONS, |r| {
                io::list(r, 4, |r| {
                    Ok(Faction {
                        form: r.ref_id()?,
                        rank: r.u8()? as i8,
                    })
                })
            })?,
            spells: io::optional(&mut r, flags, flags::SPELLS, |r| {
                Ok(Spells {
                    spells: io::references(r)?,
                    leveled_spells: io::references(r)?,
                    shouts: io::references(r)?,
                })
            })?,
            ai: io::optional(&mut r, flags, flags::AI, io::bytes)?,
            name: io::optional(&mut r, flags, flags::NAME, |r| r.wstring())?,
            skills: io::optional(&mut r, flags, flags::SKILLS, io::bytes)?,
            class: io::optional(&mut r, flags, flags::CLASS, |r| r.ref_id())?,
            race: io::optional(&mut r, flags, flags::RACE, |r| {
                Ok((r.ref_id()?, r.ref_id()?))
            })?,
            face: io::optional(&mut r, flags, flags::FACE, Face::read)?,
            gender: io::optional(&mut r, flags, flags::GENDER, |r| r.u8())?,
            default_outfit: io::optional(&mut r, flags, flags::DEFAULT_OUTFIT, |r| r.ref_id())?,
            sleep_outfit: io::optional(&mut r, flags, flags::SLEEP_OUTFIT, |r| r.ref_id())?,
        };
        Ok((value, r.pos()))
    }

    /// Rebuild the sequence from named fields, checking their flag presence.
    ///
    /// # Errors
    /// Unknown flags, mismatched presence, list count changes or oversized arrays.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut present = self.change_flags & flags::ATTRIBUTES;
        for (flag, set) in [
            (flags::FORM, self.form_flags.is_some()),
            (flags::BASE, self.base.is_some()),
            (flags::FACTIONS, self.factions.is_some()),
            (flags::SPELLS, self.spells.is_some()),
            (flags::AI, self.ai.is_some()),
            (flags::NAME, self.name.is_some()),
            (flags::SKILLS, self.skills.is_some()),
            (flags::CLASS, self.class.is_some()),
            (flags::RACE, self.race.is_some()),
            (flags::FACE, self.face.is_some()),
            (flags::GENDER, self.gender.is_some()),
            (flags::DEFAULT_OUTFIT, self.default_outfit.is_some()),
            (flags::SLEEP_OUTFIT, self.sleep_outfit.is_some()),
        ] {
            if set {
                present |= flag;
            }
        }
        if present != self.change_flags {
            return Err(invalid("NPC flags disagree with field presence"));
        }
        let mut out = vec![];
        if let Some((flags, unknown)) = self.form_flags {
            out.extend(flags.to_le_bytes());
            out.extend(unknown.to_le_bytes());
        }
        if let Some(bytes) = self.base {
            out.extend(bytes);
        }
        if let Some(factions) = &self.factions {
            factions.write(&mut out, |entry, out| {
                entry.form.write(out);
                out.push(entry.rank as u8);
            })?;
        }
        if let Some(spells) = &self.spells {
            for list in [&spells.spells, &spells.leveled_spells, &spells.shouts] {
                list.write(&mut out, |id, out| id.write(out))?;
            }
        }
        if let Some(bytes) = self.ai {
            out.extend(bytes);
        }
        if let Some(name) = &self.name {
            name.write(&mut out);
        }
        if let Some(bytes) = self.skills {
            out.extend(bytes);
        }
        if let Some(id) = self.class {
            id.write(&mut out);
        }
        if let Some((new, old)) = self.race {
            new.write(&mut out);
            old.write(&mut out);
        }
        if let Some(face) = &self.face {
            face.write(&mut out)?;
        }
        if let Some(gender) = self.gender {
            out.push(gender);
        }
        for id in [self.default_outfit, self.sleep_outfit]
            .into_iter()
            .flatten()
        {
            id.write(&mut out);
        }
        Ok(out)
    }
}

impl ChangeForm {
    /// Decode a complete NPC_ payload. Other change-form types return None.
    ///
    /// # Errors
    /// Decompression errors, malformed NPC fields or any undecoded trailing bytes.
    pub fn npc_change(&self) -> Result<Option<NpcChange>> {
        if self.type_index() != 9 {
            return Ok(None);
        }
        let payload = self.payload()?;
        let (value, used) = NpcChange::read_prefix(&payload, self.change_flags)?;
        if used != payload.len() {
            return Err(invalid(format!(
                "{} trailing bytes after NPC fields",
                payload.len() - used
            )));
        }
        Ok(Some(value))
    }
}
