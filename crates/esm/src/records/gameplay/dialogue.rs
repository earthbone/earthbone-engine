// SPDX-License-Identifier: GPL-3.0-or-later

//! DIAL, INFO, DLBR and SCEN fields from xEdit and UESP. DIAL subtype follows
//! file order. INFO topic ownership is in its group, not its TPIC
//! previous-topic field. SCEN's Phases, Actors and Actions lists carry no
//! counts, each list ends where the next one's leading field begins.

use super::{check_tag, fields, optional_u32, text, text_bytes, Condition, Result, Text};
use crate::cursor::zstring;
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

/// Scene branch selection metadata. Category and flags stay numeric, as in
/// the runtime record loader.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueBranch {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub quest: Option<u32>,
    /// TNAM. 0 is Player, 1 is Favor.
    pub category: Option<u32>,
    /// DNAM. Bits 0 to 2 are Top-Level, Blocking, Exclusive.
    pub flags: Option<u32>,
    pub starting_topic: Option<u32>,
}

impl DialogueBranch {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type or a malformed field length.
    pub fn from_record(rec: &Record) -> Result<Self> {
        check_tag(rec, *b"DLBR")?;
        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            quest: optional_u32(rec, *b"QNAM")?,
            category: optional_u32(rec, *b"TNAM")?,
            flags: optional_u32(rec, *b"DNAM")?,
            starting_topic: optional_u32(rec, *b"SNAM")?,
        })
    }
}

/// One HNAM to HNAM phase: its name, start and completion conditions split at
/// the first NEXT marker, and its editor display width.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScenePhase {
    pub name: Option<String>,
    pub start_conditions: Vec<Condition>,
    pub completion_conditions: Vec<Condition>,
    pub width: Option<u32>,
}

/// One scene participant slot. LNAM and DNAM are absent in some older records.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneActor {
    /// The alias ID this slot fills, or -1 for none.
    pub alias_id: i32,
    pub flags: Option<u32>,
    pub behavior_flags: Option<u32>,
}

/// The type-specific fields an action's ANAM discriminator selects.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneActionData {
    Dialogue {
        topic: Option<u32>,
    },
    Package {
        packages: Vec<u32>,
    },
    Timer {
        duration: Option<f32>,
    },
    /// `action_type` names a type this view has no union member for, or the
    /// type-specific subrecord was absent.
    Unknown,
}

/// One ANAM to ANAM action block.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneAction {
    /// 0 is Dialogue, 1 is Package, 2 is Timer.
    pub action_type: u16,
    pub name: Option<String>,
    pub alias_id: Option<i32>,
    pub index: Option<u32>,
    /// Bit 15 is Face Target, 16 is Looping, 17 is Headtrack Player.
    pub flags: Option<u32>,
    pub start_phase: Option<u32>,
    pub end_phase: Option<u32>,
    pub data: SceneActionData,
}

/// SCEN metadata, ordered phases, actors and actions, and scene-level
/// conditions. VMAD stays raw, its fragment section belongs to FMT-03.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub vmad: Option<Vec<u8>>,
    /// Bits 0 to 4 are Begin-on-quest-start, Stop-quest-on-end,
    /// Show-all-text, Repeat-conditions-while-true, Interruptible.
    pub flags: Option<u32>,
    pub phases: Vec<ScenePhase>,
    pub actors: Vec<SceneActor>,
    pub actions: Vec<SceneAction>,
    pub quest: Option<u32>,
    pub last_action_index: Option<u32>,
    /// Death, Combat, Dialogue, Observe-combat "set all" selections.
    pub behavior_settings: Option<[u32; 4]>,
    pub conditions: Vec<Condition>,
}

/// SCEN's Phases, Actors and Actions lists share no field names except at
/// their boundaries, so the record is read as one pass through these
/// sections in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSection {
    Header,
    Phases,
    Actors,
    Actions,
    Trailer,
}

fn new_action(action_type: u16) -> SceneAction {
    SceneAction {
        action_type,
        name: None,
        alias_id: None,
        index: None,
        flags: None,
        start_phase: None,
        end_phase: None,
        data: SceneActionData::Unknown,
    }
}

/// Settle a Timer action's held SNAM when its block closes. Only one SNAM
/// arrived and no ENAM followed it, so position gives no answer, and the
/// view keeps the value as the Duration that makes the action a Timer.
fn close_action(mut action: SceneAction, held_snam: Option<u32>) -> SceneAction {
    if let Some(value) = held_snam {
        action.data = SceneActionData::Timer {
            duration: Some(f32::from_bits(value)),
        };
    }
    action
}

impl Scene {
    /// Read this view without changing the source record.
    ///
    /// # Errors
    ///
    /// Returns an error for the wrong record type, a malformed field length
    /// or a condition subrecord without a preceding CTDA.
    #[allow(clippy::too_many_lines)]
    pub fn from_record(rec: &Record) -> Result<Self> {
        check_tag(rec, *b"SCEN")?;

        let mut section = SceneSection::Header;
        let mut flags = None;

        let mut phases: Vec<ScenePhase> = Vec::new();
        let mut current_phase: Option<ScenePhase> = None;
        let mut phase_next_count: u8 = 0;

        let mut actors: Vec<SceneActor> = Vec::new();
        let mut current_actor: Option<SceneActor> = None;

        let mut actions: Vec<SceneAction> = Vec::new();
        let mut current_action: Option<SceneAction> = None;
        // SNAM is Start Phase for every action type. Timer actions carry a
        // second SNAM, Duration, from the type-specific union after ENAM
        // (TES5Edit wbDefinitionsTES5.pas lines 7343 and 7361,
        // uesp-modfileformat SCEN.wikitext line 153). A Timer's first SNAM
        // is held here until ENAM or a second SNAM shows it was Start Phase.
        // A SNAM still held when the action closes is the only one, and the
        // view keeps it as the Timer's Duration.
        let mut timer_snam: Option<u32> = None;
        let mut action_enam_seen = false;

        let mut quest = None;
        let mut last_action_index = None;
        let mut behavior_settings = None;
        let mut conditions = Vec::new();

        for sub in &rec.subrecords {
            let tag = sub.tag.as_bytes();
            let empty = sub.data.is_empty();

            // The first field of the next section, seen while still in an
            // earlier, empty one, opens that next section here so the match
            // below can handle it uniformly.
            if section == SceneSection::Header {
                match tag {
                    b"HNAM" if empty => section = SceneSection::Phases,
                    b"ALID" => section = SceneSection::Actors,
                    b"ANAM" if !empty => section = SceneSection::Actions,
                    b"PNAM" => section = SceneSection::Trailer,
                    _ => {}
                }
            }

            match section {
                SceneSection::Header => {
                    if tag == b"FNAM" {
                        flags = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                    }
                }
                SceneSection::Phases => match tag {
                    b"HNAM" if empty => {
                        if let Some(phase) = current_phase.take() {
                            phases.push(phase);
                        } else {
                            current_phase = Some(ScenePhase::default());
                            phase_next_count = 0;
                        }
                    }
                    b"NAM0" => {
                        if let Some(phase) = current_phase.as_mut() {
                            phase.name = Some(zstring(&sub.data));
                        }
                    }
                    b"CTDA" | b"CIS1" | b"CIS2" => {
                        if let Some(phase) = current_phase.as_mut() {
                            let target = if phase_next_count == 0 {
                                &mut phase.start_conditions
                            } else {
                                &mut phase.completion_conditions
                            };
                            Condition::push(rec, sub, target)?;
                        }
                    }
                    b"NEXT" if empty => phase_next_count = phase_next_count.saturating_add(1),
                    b"WNAM" => {
                        if let Some(phase) = current_phase.as_mut() {
                            phase.width = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    // SCHR/SCDA/SCTX/SCRO/QNAM here are leftover CK-internal
                    // scripting fields from pre-SE phases, unused and ignored.
                    // Each transition is driven by a field only the next
                    // section defines, so an unclosed phase ends here too.
                    b"ALID" => {
                        if let Some(phase) = current_phase.take() {
                            phases.push(phase);
                        }
                        section = SceneSection::Actors;
                        current_actor = Some(SceneActor {
                            alias_id: fields(rec, sub.tag, &sub.data, 4)?.i32()?,
                            flags: None,
                            behavior_flags: None,
                        });
                    }
                    b"ANAM" if !empty => {
                        if let Some(phase) = current_phase.take() {
                            phases.push(phase);
                        }
                        section = SceneSection::Actions;
                        timer_snam = None;
                        action_enam_seen = false;
                        current_action =
                            Some(new_action(fields(rec, sub.tag, &sub.data, 2)?.u16()?));
                    }
                    b"PNAM" => {
                        if let Some(phase) = current_phase.take() {
                            phases.push(phase);
                        }
                        section = SceneSection::Trailer;
                        quest = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                    }
                    _ => {}
                },
                SceneSection::Actors => match tag {
                    b"ALID" => {
                        if let Some(actor) = current_actor.take() {
                            actors.push(actor);
                        }
                        current_actor = Some(SceneActor {
                            alias_id: fields(rec, sub.tag, &sub.data, 4)?.i32()?,
                            flags: None,
                            behavior_flags: None,
                        });
                    }
                    b"LNAM" => {
                        if let Some(actor) = current_actor.as_mut() {
                            actor.flags = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    b"DNAM" => {
                        if let Some(actor) = current_actor.as_mut() {
                            actor.behavior_flags = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    b"ANAM" if !empty => {
                        if let Some(actor) = current_actor.take() {
                            actors.push(actor);
                        }
                        section = SceneSection::Actions;
                        timer_snam = None;
                        action_enam_seen = false;
                        current_action =
                            Some(new_action(fields(rec, sub.tag, &sub.data, 2)?.u16()?));
                    }
                    b"PNAM" => {
                        if let Some(actor) = current_actor.take() {
                            actors.push(actor);
                        }
                        section = SceneSection::Trailer;
                        quest = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                    }
                    _ => {}
                },
                SceneSection::Actions => match tag {
                    b"ANAM" => {
                        if let Some(action) = current_action.take() {
                            actions.push(close_action(action, timer_snam.take()));
                        }
                        if !empty {
                            action_enam_seen = false;
                            current_action =
                                Some(new_action(fields(rec, sub.tag, &sub.data, 2)?.u16()?));
                        }
                    }
                    b"NAM0" => {
                        if let Some(action) = current_action.as_mut() {
                            action.name = Some(zstring(&sub.data));
                        }
                    }
                    b"ALID" => {
                        if let Some(action) = current_action.as_mut() {
                            action.alias_id = Some(fields(rec, sub.tag, &sub.data, 4)?.i32()?);
                        }
                    }
                    b"INAM" => {
                        if let Some(action) = current_action.as_mut() {
                            action.index = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        } else {
                            section = SceneSection::Trailer;
                            last_action_index = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    b"FNAM" => {
                        if let Some(action) = current_action.as_mut() {
                            action.flags = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    b"SNAM" => {
                        if let Some(action) = current_action.as_mut() {
                            let value = fields(rec, sub.tag, &sub.data, 4)?.u32()?;
                            if action.action_type != 2 {
                                action.start_phase = Some(value);
                            } else if action_enam_seen || action.start_phase.is_some() {
                                // After ENAM or a first SNAM: the Timer
                                // union's Duration, not another Start Phase.
                                action.data = SceneActionData::Timer {
                                    duration: Some(f32::from_bits(value)),
                                };
                            } else if let Some(first) = timer_snam.replace(value) {
                                // A second SNAM with no ENAM between: the
                                // held one was Start Phase, this is Duration.
                                timer_snam = None;
                                action.start_phase = Some(first);
                                action.data = SceneActionData::Timer {
                                    duration: Some(f32::from_bits(value)),
                                };
                            }
                        }
                    }
                    b"ENAM" => {
                        if let Some(action) = current_action.as_mut() {
                            action.end_phase = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                            if let Some(first) = timer_snam.take() {
                                action.start_phase = Some(first);
                            }
                            action_enam_seen = true;
                        }
                    }
                    b"DATA" => {
                        if let Some(action) = current_action.as_mut() {
                            if action.action_type == 0 {
                                action.data = SceneActionData::Dialogue {
                                    topic: Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?),
                                };
                            }
                        }
                    }
                    b"PNAM" => {
                        // TES5Edit's wbRUnion scopes PNAM to Package actions
                        // (type 1) as it scopes DATA to Dialogue (type 0). A
                        // PNAM inside any other open action is ignored.
                        if let Some(action) = current_action.as_mut() {
                            if action.action_type != 1 {
                                continue;
                            }
                            let package = fields(rec, sub.tag, &sub.data, 4)?.u32()?;
                            if let SceneActionData::Package { packages } = &mut action.data {
                                packages.push(package);
                            } else {
                                action.data = SceneActionData::Package {
                                    packages: vec![package],
                                };
                            }
                        } else {
                            section = SceneSection::Trailer;
                            quest = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?);
                        }
                    }
                    b"VNAM" if current_action.is_none() => {
                        section = SceneSection::Trailer;
                        let mut f = fields(rec, sub.tag, &sub.data, 16)?;
                        behavior_settings = Some([f.u32()?, f.u32()?, f.u32()?, f.u32()?]);
                    }
                    // HTID/DMAX/DMIN/DEMO/DEVA are dialogue-action extras
                    // this view does not decode. SCHR/SCDA/SCTX/SCRO/QNAM
                    // are the same pre-SE leftover fields as in Phases.
                    _ => {}
                },
                SceneSection::Trailer => match tag {
                    b"PNAM" => quest = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?),
                    b"INAM" => last_action_index = Some(fields(rec, sub.tag, &sub.data, 4)?.u32()?),
                    b"VNAM" => {
                        let mut f = fields(rec, sub.tag, &sub.data, 16)?;
                        behavior_settings = Some([f.u32()?, f.u32()?, f.u32()?, f.u32()?]);
                    }
                    b"CTDA" | b"CIS1" | b"CIS2" => {
                        Condition::push(rec, sub, &mut conditions)?;
                    }
                    _ => {}
                },
            }
        }

        if let Some(phase) = current_phase {
            phases.push(phase);
        }
        if let Some(actor) = current_actor {
            actors.push(actor);
        }
        if let Some(action) = current_action {
            actions.push(close_action(action, timer_snam));
        }

        Ok(Self {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            vmad: rec.find(Tag::new(b"VMAD")).map(<[u8]>::to_vec),
            flags,
            phases,
            actors,
            actions,
            quest,
            last_action_index,
            behavior_settings,
            conditions,
        })
    }
}
