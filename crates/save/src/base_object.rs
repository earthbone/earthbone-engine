// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared base-object change fields from xEdit `wbDefinitionsTES5Saves.pas`.
//!
//! xEdit assigns the value and full-name unions to zero-sized `wbNull` fields.
//! `wbNull` is an `Unused` byte array with size `-255`, which xEdit explicitly
//! resolves to zero bytes in `TwbByteArrayDef.GetSize`.

use crate::cursor::Cursor;
use crate::{ChangeForm, Result};

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

/// Decoded ARMO, WEAP or AMMO change payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemChange {
    /// The record type selected by the saved change-form type index.
    pub record_type: &'static str,
    /// The six-byte mask-invert and unknown-word section when bit zero is set.
    pub form_flags: Option<(u32, u16)>,
    /// Whether the zero-byte base-object value field is flagged as changed.
    pub value_changed: bool,
    /// Whether the zero-byte base-object full-name field is flagged as changed.
    pub full_name_changed: bool,
    /// Flag bits beyond the three named base-object flags.
    pub unnamed_flags: u32,
    /// Bytes after the source-defined fields, preserved without interpretation.
    pub trailing_data: Vec<u8>,
}

impl ItemChange {
    /// Rebuild the saved change flag word represented by these fields.
    #[must_use]
    pub fn change_flags(&self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed) << 1)
            | (u32::from(self.full_name_changed) << 2)
            | self.unnamed_flags
    }

    /// Rebuild the typed fields followed by any retained unclassified bytes.
    #[must_use]
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 + self.trailing_data.len());
        if let Some((mask, unknown)) = self.form_flags {
            out.extend(mask.to_le_bytes());
            out.extend(unknown.to_le_bytes());
        }
        out.extend_from_slice(&self.trailing_data);
        out
    }
}

const BASE_OBJECT_TYPES: &[&str] = &[
    "ACTI", "TACT", "ARMO", "BOOK", "CONT", "DOOR", "LIGH", "MISC", "APPA", "STAT", "MSTT", "FURN",
    "WEAP", "AMMO", "KEYM", "ALCH", "IDLM", "NAVM", "WOOP", "MGEF", "ENCH",
];

impl ChangeForm {
    /// Decode the shared saved fields for ARMO, WEAP and AMMO.
    ///
    /// xEdit defines the value and full-name flag alternatives as zero-byte
    /// fields. Unnamed flag bits and bytes after the defined fields are kept.
    ///
    /// # Errors
    /// Returns a payload decompression error or a truncation error when the
    /// optional six-byte form-flags section is incomplete.
    pub fn item_change(&self) -> Result<Option<ItemChange>> {
        if !matches!(self.record_type(), "ARMO" | "WEAP" | "AMMO") {
            return Ok(None);
        }

        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "item change");
        let form_flags = if self.change_flags & flags::FORM != 0 {
            Some((cursor.u32()?, cursor.u16()?))
        } else {
            None
        };
        let trailing_data = cursor.rest().to_vec();
        Ok(Some(ItemChange {
            record_type: self.record_type(),
            form_flags,
            value_changed: self.change_flags & flags::VALUE != 0,
            full_name_changed: self.change_flags & flags::FULL_NAME != 0,
            unnamed_flags: self.change_flags & !0b111,
            trailing_data,
        }))
    }

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
}
