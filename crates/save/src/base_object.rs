// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared base-object change fields from xEdit `wbDefinitionsTES5Saves.pas`.
//!
//! xEdit assigns the value and full-name unions to zero-sized `wbNull` fields.
//! `wbNull` is an `Unused` byte array with size `-255`, which xEdit explicitly
//! resolves to zero bytes in `TwbByteArrayDef.GetSize`.

use crate::cursor::Cursor;
use crate::{ChangeForm, Error, Result};

/// Change flag masks for the shared base-object prefix.
pub mod flags {
    /// The six-byte mask-invert and unknown-word section.
    pub const FORM: u32 = 1;
    /// xEdit's zero-byte `CHANGE_BASE_OBJECT_VALUE` field.
    pub const VALUE: u32 = 1 << 1;
    /// xEdit's zero-byte `CHANGE_BASE_OBJECT_FULLNAME` field.
    pub const FULL_NAME: u32 = 1 << 2;
}

/// Decoded prefix fields shared by Skyrim base-object change forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseObjectFields {
    /// The form flag mask and following 16-bit value when bit zero is set.
    pub form_flags: Option<(u32, u16)>,
    /// Whether the zero-byte base-object value field is flagged as changed.
    pub value_changed: bool,
    /// Whether the zero-byte full-name field is flagged as changed.
    pub full_name_changed: bool,
    /// Number of payload bytes consumed by this shared prefix.
    pub bytes_consumed: usize,
    /// Bytes belonging to later type-specific fields.
    pub remaining_bytes: usize,
}

const BASE_OBJECT_TYPES: &[&str] = &[
    "ACTI", "TACT", "ARMO", "BOOK", "CONT", "DOOR", "LIGH", "MISC", "APPA", "STAT", "MSTT", "FURN",
    "WEAP", "AMMO", "KEYM", "ALCH", "IDLM", "NAVM", "WOOP", "MGEF", "ENCH",
];

impl ChangeForm {
    /// Decode the shared base-object fields before a type-specific suffix.
    ///
    /// xEdit orders the optional six-byte form flags before the value and
    /// full-name fields. The latter consume zero bytes whether their flags are
    /// set or clear.
    ///
    /// # Errors
    /// Returns a payload decompression error or a truncation error when the
    /// six-byte form-flags section is present but incomplete.
    pub fn base_object_fields(&self) -> Result<Option<BaseObjectFields>> {
        if !BASE_OBJECT_TYPES.contains(&self.record_type()) {
            return Ok(None);
        }

        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "base-object change");
        let form_flags = if self.change_flags & flags::FORM != 0 {
            Some((cursor.u32()?, cursor.u16()?))
        } else {
            None
        };
        let bytes_consumed = cursor.pos();
        Ok(Some(BaseObjectFields {
            form_flags,
            value_changed: self.change_flags & flags::VALUE != 0,
            full_name_changed: self.change_flags & flags::FULL_NAME != 0,
            bytes_consumed,
            remaining_bytes: cursor.remaining(),
        }))
    }

    /// Decode the saved fields of an APPA record.
    ///
    /// # Errors
    /// Returns a decompression error, a truncation error, or an error when
    /// bytes remain after the fields defined by xEdit.
    ///
    /// # Panics
    /// Does not panic for malformed payloads.
    pub fn apparatus_change(&self) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != "APPA" {
            return Ok(None);
        }
        decode_base_object_change(self)
    }

    /// Decode the saved fields of an IDLM record.
    ///
    /// # Errors
    /// Returns a decompression error, a truncation error, or an error when
    /// bytes remain after the fields defined by xEdit.
    ///
    /// # Panics
    /// Does not panic for malformed payloads.
    pub fn idle_marker_change(&self) -> Result<Option<BaseObjectChange>> {
        if self.record_type() != "IDLM" {
            return Ok(None);
        }
        decode_base_object_change(self)
    }
}

/// The fields shared by APPA and IDLM change forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseObjectChange {
    /// The complete change-flags word, including unnamed bits.
    pub change_flags: u32,
    /// The optional six-byte form-flags section.
    pub form_flags: Option<(u32, u16)>,
    /// Whether xEdit's zero-byte base-object value field is flagged changed.
    pub value_changed: bool,
    /// Whether xEdit's zero-byte full-name field is flagged changed.
    pub full_name_changed: bool,
}

impl BaseObjectChange {
    /// Rebuild the payload from the decoded fields.
    ///
    /// # Panics
    /// Does not panic.
    #[must_use]
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some((flags, unknown)) = self.form_flags {
            out.extend(flags.to_le_bytes());
            out.extend(unknown.to_le_bytes());
        }
        out
    }
}

fn decode_base_object_change(change: &ChangeForm) -> Result<Option<BaseObjectChange>> {
    let payload = change.payload()?;
    let mut cursor = Cursor::new(&payload, "base-object change");
    let form_flags = if change.change_flags & flags::FORM != 0 {
        Some((cursor.u32()?, cursor.u16()?))
    } else {
        None
    };
    if !cursor.is_empty() {
        return Err(Error::RecordChange {
            reason: "trailing base-object change bytes".into(),
        });
    }
    Ok(Some(BaseObjectChange {
        change_flags: change.change_flags,
        form_flags,
        value_changed: change.change_flags & flags::VALUE != 0,
        full_name_changed: change.change_flags & flags::FULL_NAME != 0,
    }))
}
