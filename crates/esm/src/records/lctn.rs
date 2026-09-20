// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed view of an `LCTN` record. Layout from xEdit's `wbRecord(LCTN)` in
//! `wbDefinitionsTES5.pas`. The view retains the parent location and world
//! marker that package destinations use.

use crate::{Error, Record, Result, Tag};

use super::{f32_field, form_id};

/// The location record tag.
pub const LCTN: Tag = Tag::new(b"LCTN");

/// The package-relevant fields of an `LCTN` record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    /// The form id as written in the plugin.
    pub form_id: u32,
    /// `PNAM`, the parent location when present.
    pub parent: Option<u32>,
    /// `MNAM`, the reference that marks this location in the world.
    pub world_marker: Option<u32>,
    /// `RNAM`, the radius around `world_marker`.
    pub world_radius: Option<f32>,
}

impl Location {
    /// Reads the package-relevant fields of an `LCTN` record.
    ///
    /// # Errors
    ///
    /// Returns an error when `rec` is not an `LCTN` record or a modeled
    /// FormID or float subrecord has the wrong size.
    pub fn from_record(rec: &Record) -> Result<Self> {
        if rec.tag != LCTN {
            return Err(Error::Invalid(format!("{} is not an LCTN record", rec.tag)));
        }
        Ok(Self {
            form_id: rec.form_id,
            parent: form_id(rec, Tag::new(b"PNAM"))?,
            world_marker: form_id(rec, Tag::new(b"MNAM"))?,
            world_radius: f32_field(rec, Tag::new(b"RNAM"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_view_reads_package_destination_fields() {
        let record = Record::new(LCTN, 0x123)
            .sub(Tag::new(b"PNAM"), 0x456_u32.to_le_bytes())
            .sub(Tag::new(b"MNAM"), 0x789_u32.to_le_bytes())
            .sub(Tag::new(b"RNAM"), 512.0_f32.to_le_bytes());

        assert_eq!(
            Location::from_record(&record).unwrap(),
            Location {
                form_id: 0x123,
                parent: Some(0x456),
                world_marker: Some(0x789),
                world_radius: Some(512.0),
            }
        );
    }

    #[test]
    fn location_view_rejects_wrong_record_kind_and_field_sizes() {
        let wrong_kind = Record::new(Tag::new(b"CELL"), 1);
        assert!(Location::from_record(&wrong_kind).is_err());

        let short_marker = Record::new(LCTN, 1).sub(Tag::new(b"MNAM"), [0_u8; 3]);
        assert!(Location::from_record(&short_marker).is_err());
    }
}
