// SPDX-License-Identifier: GPL-3.0-or-later

//! HAZD hazard data from TES5Edit's record definition, checked against UESP,
//! Mutagen and CommonLibSSE-NG. The 40-byte DATA block stores four raw links.

use crate::cursor::Fields;
use crate::error::{Error, Result};
use crate::plugin::Record;
use crate::tag::Tag;

/// HAZD DATA flag bits. Unknown bits remain available in [`Hazard::flags`].
pub mod flags {
    /// Affects the player only.
    pub const AFFECTS_PLAYER_ONLY: u32 = 0x01;
    /// Inherit duration from the spawning spell.
    pub const INHERIT_DURATION: u32 = 0x02;
    /// Align to the impact normal.
    pub const ALIGN_TO_IMPACT_NORMAL: u32 = 0x04;
    /// Inherit radius from the spawning spell.
    pub const INHERIT_RADIUS: u32 = 0x08;
    /// Drop the hazard to the ground.
    pub const DROP_TO_GROUND: u32 = 0x10;
}

/// Typed fields from one HAZD record. FormIDs remain raw and owner-relative.
#[derive(Debug, Clone, PartialEq)]
pub struct Hazard {
    /// Record FormID as written in its plugin.
    pub form_id: u32,
    /// Record flags from the record header.
    pub record_flags: u32,
    /// Editor identifier, when present.
    pub editor_id: Option<String>,
    /// DATA limit count.
    pub limit: u32,
    /// DATA radius.
    pub radius: f32,
    /// DATA lifetime.
    pub lifetime: f32,
    /// DATA image-space modifier radius.
    pub image_space_radius: f32,
    /// DATA target interval.
    pub target_interval: f32,
    /// DATA flags, including unknown bits.
    pub flags: u32,
    /// DATA spell FormID, which may identify SPEL or ENCH.
    pub spell: u32,
    /// DATA light FormID.
    pub light: u32,
    /// DATA impact data set FormID.
    pub impact_data_set: u32,
    /// DATA sound descriptor FormID.
    pub sound: u32,
    /// MNAM image-space modifier FormID, when present.
    pub image_space_modifier: Option<u32>,
}

impl Hazard {
    /// Read the named fields without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, a missing or duplicate DATA,
    /// or malformed DATA and MNAM sizes.
    pub fn from_record(rec: &Record) -> Result<Self> {
        if rec.tag != Tag::new(b"HAZD") {
            return Err(Error::Invalid(format!("expected HAZD, found {}", rec.tag)));
        }
        let mut data_subrecords = rec.all(Tag::new(b"DATA"));
        let data = data_subrecords.next().ok_or(Error::MissingSubrecord {
            record: rec.tag,
            tag: Tag::new(b"DATA"),
        })?;
        if data_subrecords.next().is_some() {
            return Err(Error::Invalid("HAZD has duplicate DATA subrecords".into()));
        }
        let mut fields = Fields::new(rec.tag, Tag::new(b"DATA"), data);
        fields.exact(40)?;
        let limit = fields.u32()?;
        let radius = fields.f32()?;
        let lifetime = fields.f32()?;
        let image_space_radius = fields.f32()?;
        let target_interval = fields.f32()?;
        let flags = fields.u32()?;
        let spell = fields.u32()?;
        let light = fields.u32()?;
        let impact_data_set = fields.u32()?;
        let sound = fields.u32()?;
        let mut image_space_subrecords = rec.all(Tag::new(b"MNAM"));
        let image_space_modifier = match image_space_subrecords.next() {
            None => None,
            Some(bytes) => {
                if image_space_subrecords.next().is_some() {
                    return Err(Error::Invalid("HAZD has duplicate MNAM subrecords".into()));
                }
                let mut field = Fields::new(rec.tag, Tag::new(b"MNAM"), bytes);
                field.exact(4)?;
                Some(field.u32()?)
            }
        };
        Ok(Self {
            form_id: rec.form_id,
            record_flags: rec.flags,
            editor_id: rec.editor_id(),
            limit,
            radius,
            lifetime,
            image_space_radius,
            target_interval,
            flags,
            spell,
            light,
            impact_data_set,
            sound,
            image_space_modifier,
        })
    }
}
