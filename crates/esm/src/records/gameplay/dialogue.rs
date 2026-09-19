// SPDX-License-Identifier: GPL-3.0-or-later

//! DIAL and INFO fields from xEdit and UESP. DIAL subtype follows file order.
//! INFO topic ownership is in its group, not its TPIC previous-topic field.

use super::{check_tag, fields, optional_u32, text, text_bytes, Condition, Result, Text};
use crate::{Error, Record, Tag};

/// The final subtype field encountered while reading a DIAL record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicSubtype {
    /// Legacy DATA numeric subtype. The meaning varies with game version.
    Numeric(u16),
    /// SNAM signature, retained without converting it through a versioned table.
    Signature([u8; 4]),
}

/// Topic ownership, selection metadata and player-visible text.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueTopic {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub priority: Option<f32>,
    pub branch: Option<u32>,
    pub quest: Option<u32>,
    pub do_all_before_repeating: Option<u8>,
    pub category: Option<u8>,
    /// The last DATA or SNAM field wins, as in the runtime record loader.
    pub subtype: Option<TopicSubtype>,
    pub declared_info_count: Option<u32>,
}

impl DialogueTopic {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record, localized: bool) -> Result<Self> {
        check_tag(rec, *b"DIAL")?;
        let mut do_all_before_repeating = None;
        let mut category = None;
        let mut subtype = None;
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"DATA" => {
                    let mut f = fields(rec, sub.tag, &sub.data, 4)?;
                    do_all_before_repeating = Some(f.u8()?);
                    category = Some(f.u8()?);
                    subtype = Some(TopicSubtype::Numeric(f.u16()?));
                }
                b"SNAM" => {
                    let mut f = fields(rec, sub.tag, &sub.data, 4)?;
                    subtype = Some(TopicSubtype::Signature(f.u32()?.to_le_bytes()));
                }
                _ => {}
            }
        }
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: text(rec, *b"FULL", localized)?,
            priority: super::super::f32_field(rec, Tag::new(b"PNAM"))?,
            branch: optional_u32(rec, *b"BNAM")?,
            quest: optional_u32(rec, *b"QNAM")?,
            do_all_before_repeating,
            category,
            subtype,
            declared_info_count: optional_u32(rec, *b"TIFC")?,
        })
    }
}

/// Legacy reset days or packed modern reset hours. Raw flags are retained.
#[derive(Debug, Clone, PartialEq)]
pub enum InfoData {
    Legacy {
        category: u16,
        flags: u16,
        reset_days: f32,
    },
    Modern {
        flags: u16,
        reset_hours: u16,
    },
}

impl InfoData {
    /// Response flags in either on-disk representation.
    pub fn flags(&self) -> u16 {
        match self {
            Self::Legacy { flags, .. } | Self::Modern { flags, .. } => *flags,
        }
    }
    /// Reset interval in hours. Modern ENAM stores 2730.625 units per hour.
    pub fn reset_hours(&self) -> f32 {
        match self {
            Self::Legacy { reset_days, .. } => reset_days * 24.0,
            Self::Modern { reset_hours, .. } => f32::from(*reset_hours) / 2730.625,
        }
    }
}

/// One TRDT response and its following text and idle fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub emotion: u32,
    pub emotion_value: u32,
    pub number: u8,
    pub sound: u32,
    pub use_emotion_animation: u8,
    pub text: Option<Text>,
    pub speaker_idle: Option<u32>,
    pub listener_idle: Option<u32>,
}

/// INFO metadata, ordered responses, conditions and raw script attachment data.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueInfo {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub data: Option<InfoData>,
    pub previous_topic: Option<u32>,
    pub previous_info: Option<u32>,
    pub shared_info: Option<u32>,
    pub topic_links: Vec<u32>,
    pub favor_level: Option<u8>,
    pub prompt: Option<Text>,
    pub speaker: Option<u32>,
    pub walk_away_topic: Option<u32>,
    pub audio_output: Option<u32>,
    pub responses: Vec<Response>,
    pub conditions: Vec<Condition>,
    /// Unparsed VMAD, including the INFO fragment section.
    pub vmad: Option<Vec<u8>>,
}

impl DialogueInfo {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, malformed field lengths or
    /// missing required fields and malformed repeated groups.
    pub fn from_record(rec: &Record, localized: bool) -> Result<Self> {
        check_tag(rec, *b"INFO")?;
        let mut data = None;
        let mut responses: Vec<Response> = Vec::new();
        for sub in &rec.subrecords {
            match sub.tag.as_bytes() {
                b"DATA" | b"ENAM" => {
                    if data.is_some() {
                        return Err(Error::Invalid("INFO has repeated DATA/ENAM".into()));
                    }
                    data = Some(if sub.tag.as_bytes() == b"DATA" {
                        let mut f = fields(rec, sub.tag, &sub.data, 8)?;
                        InfoData::Legacy {
                            category: f.u16()?,
                            flags: f.u16()?,
                            reset_days: f.f32()?,
                        }
                    } else {
                        let mut f = fields(rec, sub.tag, &sub.data, 4)?;
                        InfoData::Modern {
                            flags: f.u16()?,
                            reset_hours: f.u16()?,
                        }
                    });
                }
                b"TRDT" => {
                    let mut f = fields(rec, sub.tag, &sub.data, 24)?;
                    let emotion = f.u32()?;
                    let emotion_value = f.u32()?;
                    f.skip(4)?;
                    let number = f.u8()?;
                    f.skip(3)?;
                    responses.push(Response {
                        emotion,
                        emotion_value,
                        number,
                        sound: f.u32()?,
                        use_emotion_animation: f.u8()?,
                        text: None,
                        speaker_idle: None,
                        listener_idle: None,
                    });
                }
                b"NAM1" | b"SNAM" | b"LNAM" => {
                    let response = responses
                        .last_mut()
                        .ok_or_else(|| Error::Invalid(format!("INFO {} without TRDT", sub.tag)))?;
                    match sub.tag.as_bytes() {
                        b"NAM1" => {
                            response.text = Some(text_bytes(rec, sub.tag, &sub.data, localized)?);
                        }
                        b"SNAM" => {
                            response.speaker_idle =
                                Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                        _ => {
                            response.listener_idle =
                                Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            data,
            previous_topic: optional_u32(rec, *b"TPIC")?,
            previous_info: optional_u32(rec, *b"PNAM")?,
            shared_info: optional_u32(rec, *b"DNAM")?,
            topic_links: super::super::form_id_list(rec, Tag::new(b"TCLT"))?,
            favor_level: rec
                .find(Tag::new(b"CNAM"))
                .map(|bytes| fields(rec, Tag::new(b"CNAM"), bytes, 1)?.u8())
                .transpose()?,
            prompt: text(rec, *b"RNAM", localized)?,
            speaker: optional_u32(rec, *b"ANAM")?,
            walk_away_topic: optional_u32(rec, *b"TWAT")?,
            audio_output: optional_u32(rec, *b"ONAM")?,
            responses,
            conditions: Condition::all(rec)?,
            vmad: rec.find(Tag::new(b"VMAD")).map(<[u8]>::to_vec),
        })
    }
}
