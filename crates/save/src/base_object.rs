// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared base-object change fields from xEdit `wbDefinitionsTES5Saves.pas`.
//!
//! xEdit assigns the value and full-name unions to zero-sized `wbNull` fields.
//! `wbNull` is an `Unused` byte array with size `-255`, which xEdit explicitly
//! resolves to zero bytes in `TwbByteArrayDef.GetSize`.

use crate::cursor::Cursor;
use crate::quest::FormFlags;
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

/// Change flags used by `CONT`, `MISC` and `FURN` change forms.
pub mod static_object_flags {
    /// The shared six-byte form-flags section.
    pub const FORM: u32 = super::flags::FORM;
    /// xEdit's zero-byte base-object value field.
    pub const VALUE: u32 = super::flags::VALUE;
    /// xEdit's zero-byte full-name field.
    pub const FULL_NAME: u32 = super::flags::FULL_NAME;
    /// The union of named fields in the xEdit definitions.
    pub const KNOWN: u32 = FORM | VALUE | FULL_NAME;
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

/// Fully decoded LIGH, STAT or MSTT change data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightStaticChange {
    /// The optional six-byte form flag section.
    pub form_flags: Option<FormFlags>,
    /// Whether xEdit's zero-byte base-object value marker is set.
    pub value_changed: bool,
    /// Whether xEdit's zero-byte base-object full-name marker is set.
    pub full_name_changed: bool,
}

impl LightStaticChange {
    /// Rebuild the defined change flag bits.
    ///
    /// # Panics
    /// Does not panic.
    #[must_use]
    pub fn change_flags(self) -> u32 {
        u32::from(self.form_flags.is_some())
            | (u32::from(self.value_changed) << 1)
            | (u32::from(self.full_name_changed) << 2)
    }

    /// Rebuild the exact source-defined payload bytes.
    ///
    /// # Panics
    /// Does not panic.
    #[must_use]
    pub fn data_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(if self.form_flags.is_some() { 6 } else { 0 });
        if let Some(flags) = self.form_flags {
            out.extend(flags.flags.to_le_bytes());
            out.extend(flags.unknown.to_le_bytes());
        }
        out
    }
}

/// Decoded `CONT`, `MISC` or `FURN` change payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticObjectChange {
    /// Original change flags, including unnamed bits.
    pub change_flags: u32,
    /// The six-byte form flags section when `FORM` is set.
    pub form_flags: Option<(u32, u16)>,
    /// Whether xEdit marks the zero-byte value field as changed.
    pub value_changed: bool,
    /// Whether xEdit marks the zero-byte full-name field as changed.
    pub full_name_changed: bool,
    /// Payload bytes after the fields defined by xEdit.
    pub opaque_suffix: Vec<u8>,
}

impl StaticObjectChange {
    /// Rebuild the payload while retaining unnamed flags and opaque bytes.
    ///
    /// # Errors
    /// Returns an error when a typed field disagrees with its change flag.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let form_present = self.change_flags & static_object_flags::FORM != 0;
        if self.form_flags.is_some() != form_present
            || self.value_changed != (self.change_flags & static_object_flags::VALUE != 0)
            || self.full_name_changed != (self.change_flags & static_object_flags::FULL_NAME != 0)
        {
            return Err(crate::Error::RecordChange {
                reason: "static-object fields disagree with change flags".to_owned(),
            });
        }

        let mut bytes =
            Vec::with_capacity(self.form_flags.map_or(0, |_| 6) + self.opaque_suffix.len());
        if let Some((form_flags, unknown)) = self.form_flags {
            bytes.extend_from_slice(&form_flags.to_le_bytes());
            bytes.extend_from_slice(&unknown.to_le_bytes());
        }
        bytes.extend_from_slice(&self.opaque_suffix);
        Ok(bytes)
    }
}

const BASE_OBJECT_TYPES: &[&str] = &[
    "ACTI", "TACT", "ARMO", "BOOK", "CONT", "DOOR", "LIGH", "MISC", "APPA", "STAT", "MSTT", "FURN",
    "WEAP", "AMMO", "KEYM", "ALCH", "IDLM", "NAVM", "WOOP", "MGEF", "ENCH",
];

impl ChangeForm {
    /// Decode a LIGH, STAT or MSTT change payload defined by xEdit.
    ///
    /// These records contain optional form flags followed by value and
    /// full-name markers. xEdit defines both markers as zero-byte fields.
    ///
    /// # Errors
    /// Returns decompression, truncation, unknown-flag or trailing-byte errors.
    ///
    /// # Panics
    /// Does not panic.
    pub fn light_static_change(&self) -> Result<Option<LightStaticChange>> {
        if !matches!(self.record_type(), "LIGH" | "STAT" | "MSTT") {
            return Ok(None);
        }
        if self.change_flags & !0b111 != 0 {
            return Err(Error::RecordChange {
                reason: "unknown LIGH/STAT/MSTT change flags".into(),
            });
        }

        let fields = self
            .base_object_fields()?
            .ok_or_else(|| Error::RecordChange {
                reason: "missing base-object fields".into(),
            })?;
        if fields.remaining_bytes != 0 {
            return Err(Error::RecordChange {
                reason: "trailing LIGH/STAT/MSTT change bytes".into(),
            });
        }

        Ok(Some(LightStaticChange {
            form_flags: fields
                .form_flags
                .map(|(flags, unknown)| FormFlags { flags, unknown }),
            value_changed: fields.value_changed,
            full_name_changed: fields.full_name_changed,
        }))
    }

    /// Decode the xEdit-defined fields in a `CONT`, `MISC` or `FURN` payload.
    ///
    /// The three records share the base-object flags. Unnamed flag bits and
    /// payload bytes beyond the defined prefix are retained without assigning
    /// them a meaning.
    ///
    /// # Errors
    /// Returns a payload decompression error or a truncation error when the
    /// six-byte form-flags section is incomplete.
    pub fn static_object_change(&self) -> Result<Option<StaticObjectChange>> {
        if !matches!(self.record_type(), "CONT" | "MISC" | "FURN") {
            return Ok(None);
        }

        let payload = self.payload()?;
        let mut cursor = Cursor::new(&payload, "static-object change");
        let form_flags = if self.change_flags & static_object_flags::FORM != 0 {
            Some((cursor.u32()?, cursor.u16()?))
        } else {
            None
        };
        Ok(Some(StaticObjectChange {
            change_flags: self.change_flags,
            form_flags,
            value_changed: self.change_flags & static_object_flags::VALUE != 0,
            full_name_changed: self.change_flags & static_object_flags::FULL_NAME != 0,
            opaque_suffix: cursor.rest().to_vec(),
        }))
    }

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

    /// Decode the saved fields of an APPA record.
    ///
    /// # Errors
    /// Returns a decompression error, a truncation error, or an error when
    /// bytes remain after the fields defined by xEdit.
    ///
    /// # Panics
    /// Does not panic for malformed payloads.
    pub fn apparatus_change(&self) -> Result<Option<AppaIdleMarkerChange>> {
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
    pub fn idle_marker_change(&self) -> Result<Option<AppaIdleMarkerChange>> {
        if self.record_type() != "IDLM" {
            return Ok(None);
        }
        decode_base_object_change(self)
    }
}

/// The fields shared by APPA and IDLM change forms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppaIdleMarkerChange {
    /// The complete change-flags word, including unnamed bits.
    pub change_flags: u32,
    /// The optional six-byte form-flags section.
    pub form_flags: Option<(u32, u16)>,
    /// Whether xEdit's zero-byte base-object value field is flagged changed.
    pub value_changed: bool,
    /// Whether xEdit's zero-byte full-name field is flagged changed.
    pub full_name_changed: bool,
}

impl AppaIdleMarkerChange {
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

fn decode_base_object_change(change: &ChangeForm) -> Result<Option<AppaIdleMarkerChange>> {
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
    Ok(Some(AppaIdleMarkerChange {
        change_flags: change.change_flags,
        form_flags,
        value_changed: change.change_flags & flags::VALUE != 0,
        full_name_changed: change.change_flags & flags::FULL_NAME != 0,
    }))
}
