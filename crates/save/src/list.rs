// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved FLST, LVLN, LVLI and LVSP changes. Field order follows FallrimTools
//! ChangeFormFLST, ChangeFormLeveled and LeveledEntry. Flag positions follow
//! xEdit wbDefinitionsTES5Saves, including bit zero for FLST form flags.

use crate::{cursor::Cursor, quest::FormFlags, ChangeForm, Error, RefId, Result};

const FORM_FLAGS: u32 = 1;
const ADDED: u32 = 1 << 31;

/// Saved additions to a form list. Order, duplicates and null RefIDs are retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormListChange {
    /// The shared mask and undocumented word, with no runtime interpretation.
    pub form_flags: Option<FormFlags>,
    /// An absent section differs from a present list containing no additions.
    pub forms: Option<Vec<RefId>>,
}

/// One seven-byte saved leveled-list addition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeveledEntry {
    /// Saved identity, resolved by the caller through the save's plugin list.
    pub form: RefId,
    /// Saved level byte.
    pub level: u8,
    /// Saved count word, retaining every bit.
    pub count: u16,
    /// Saved chance byte, retained without applying a probability policy.
    pub chance: u8,
}

/// Saved additions to an NPC, item or spell leveled list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeveledListChange {
    /// The shared mask and undocumented word, with no runtime interpretation.
    pub form_flags: Option<FormFlags>,
    /// An absent section differs from a present list containing no additions.
    pub entries: Option<Vec<LeveledEntry>>,
}

impl ChangeForm {
    /// Decode an entire FLST payload, returning `None` for other record types.
    ///
    /// # Errors
    /// Rejects unknown change bits, truncated sections and trailing bytes.
    pub fn form_list_change(&self) -> Result<Option<FormListChange>> {
        if self.record_type() != "FLST" {
            return Ok(None);
        }
        check_flags(self.change_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "form list change");
        let form_flags = read_form_flags(&mut cursor, self.change_flags)?;
        let forms = if self.change_flags & ADDED != 0 {
            let count = cursor.u32()? as usize;
            if count > cursor.remaining() / 3 {
                return Err(invalid("form count exceeds remaining payload"));
            }
            let mut forms = Vec::with_capacity(count);
            for _ in 0..count {
                forms.push(cursor.ref_id()?);
            }
            Some(forms)
        } else {
            None
        };
        finish(&cursor)?;
        Ok(Some(FormListChange { form_flags, forms }))
    }

    /// Decode an entire LVLN, LVLI or LVSP payload.
    /// Returns `None` for other record types.
    ///
    /// # Errors
    /// Rejects unknown change bits, truncated sections and trailing bytes.
    pub fn leveled_list_change(&self) -> Result<Option<LeveledListChange>> {
        if !matches!(self.record_type(), "LVLN" | "LVLI" | "LVSP") {
            return Ok(None);
        }
        check_flags(self.change_flags)?;
        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "leveled list change");
        let form_flags = read_form_flags(&mut cursor, self.change_flags)?;
        let entries = if self.change_flags & ADDED != 0 {
            let count = usize::from(cursor.u8()?);
            if count > cursor.remaining() / 7 {
                return Err(invalid("entry count exceeds remaining payload"));
            }
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                entries.push(LeveledEntry {
                    form: cursor.ref_id()?,
                    level: cursor.u8()?,
                    count: cursor.u16()?,
                    chance: cursor.u8()?,
                });
            }
            Some(entries)
        } else {
            None
        };
        finish(&cursor)?;
        Ok(Some(LeveledListChange {
            form_flags,
            entries,
        }))
    }
}

impl FormListChange {
    /// Change flags matching the current optional sections.
    pub fn change_flags(&self) -> u32 {
        u32::from(self.form_flags.is_some()) | if self.forms.is_some() { ADDED } else { 0 }
    }

    /// Rebuild the typed payload. `Save::write` still uses the original raw data.
    ///
    /// # Errors
    /// Rejects a list whose count does not fit the saved 32-bit field.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        write_form_flags(self.form_flags, &mut out);
        if let Some(forms) = &self.forms {
            let count = u32::try_from(forms.len()).map_err(|_| invalid("too many forms"))?;
            out.extend(count.to_le_bytes());
            for form in forms {
                form.write(&mut out);
            }
        }
        Ok(out)
    }
}

impl LeveledListChange {
    /// Change flags matching the current optional sections.
    pub fn change_flags(&self) -> u32 {
        u32::from(self.form_flags.is_some()) | if self.entries.is_some() { ADDED } else { 0 }
    }

    /// Rebuild the typed payload. `Save::write` still uses the original raw data.
    ///
    /// # Errors
    /// Rejects a list with more than 255 entries.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        write_form_flags(self.form_flags, &mut out);
        if let Some(entries) = &self.entries {
            let count = u8::try_from(entries.len()).map_err(|_| invalid("too many entries"))?;
            out.push(count);
            for entry in entries {
                entry.form.write(&mut out);
                out.push(entry.level);
                out.extend(entry.count.to_le_bytes());
                out.push(entry.chance);
            }
        }
        Ok(out)
    }
}

fn check_flags(flags: u32) -> Result<()> {
    if flags & !(FORM_FLAGS | ADDED) != 0 {
        return Err(invalid("unknown list change flags"));
    }
    Ok(())
}

fn read_form_flags(cursor: &mut Cursor<'_>, flags: u32) -> Result<Option<FormFlags>> {
    if flags & FORM_FLAGS == 0 {
        return Ok(None);
    }
    Ok(Some(FormFlags {
        flags: cursor.u32()?,
        unknown: cursor.u16()?,
    }))
}

fn write_form_flags(flags: Option<FormFlags>, out: &mut Vec<u8>) {
    if let Some(flags) = flags {
        out.extend(flags.flags.to_le_bytes());
        out.extend(flags.unknown.to_le_bytes());
    }
}

fn finish(cursor: &Cursor<'_>) -> Result<()> {
    if cursor.remaining() != 0 {
        return Err(invalid("trailing list change bytes"));
    }
    Ok(())
}

fn invalid(reason: &str) -> Error {
    Error::List {
        reason: reason.to_owned(),
    }
}
