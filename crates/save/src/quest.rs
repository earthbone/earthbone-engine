// SPDX-License-Identifier: GPL-3.0-or-later

//! QUST change payloads, following local UESP QUST_Changeform and xEdit's
//! wbDefinitionsTES5Saves.pas. FallrimTools ChangeFormQust confirms runtime
//! layouts but omits the instance-data section. Unknown meanings stay raw.

use crate::cursor::{Cursor, VsVal};
use crate::{ChangeForm, Error, GlobalVariable, QuestStage, RefId, Result};

mod static_data;
pub use static_data::{QuestStaticData, StaticEntry};

/// QUST header bits controlling the presence of payload sections.
pub mod flags {
    pub const FORM: u32 = 1;
    pub const QUEST: u32 = 1 << 1;
    pub const SCRIPT_DELAY: u32 = 1 << 2;
    pub const ALREADY_RUN: u32 = 1 << 26;
    pub const INSTANCES: u32 = 1 << 27;
    pub const RUNTIME: u32 = 1 << 28;
    pub const OBJECTIVES: u32 = 1 << 29;
    /// xEdit declares no payload for this bit.
    pub const SCRIPT: u32 = 1 << 30;
    pub const STAGES: u32 = 1 << 31;
    pub const KNOWN: u32 = 0xFC00_0007;
}

/// A variable-count array retaining the original count width for diagnostics.
#[derive(Clone, Debug)]
pub struct Counted<T> {
    count: VsVal,
    pub entries: Vec<T>,
}

/// Shared six-byte change-form flags, including the undocumented final word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormFlags {
    pub flags: u32,
    pub unknown: u16,
}

/// A saved objective index and its unmodified status word.
/// CommonLib's QuestObjectiveStates names states 0 through 5. Higher bits
/// remain available to callers rather than being discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Objective {
    pub id: u32,
    pub status: u32,
}

/// A runtime reference alias. Kind zero has one reference. A nonzero kind
/// has five, per UESP and FallrimTools. xEdit names kind one's fields as new
/// reference, bound object, reference location, starting location and ref type.
#[derive(Clone, Debug)]
pub struct ReferenceAlias {
    pub id: u32,
    pub kind: u8,
    pub references: Vec<RefId>,
}

/// Alias ID and reference, also used for saved runtime location aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AliasRef {
    pub id: u32,
    pub reference: RefId,
}

/// Story-event parameter tags from xEdit. Type zero is a reference there,
/// although the UESP table and FallrimTools omit it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventParameter {
    Reference { kind: u32, reference: RefId },
    Integer(u32),
}

/// Saved story event. xEdit identifies the middle four bytes as an event code,
/// whereas UESP and FallrimTools label them an unknown float.
#[derive(Clone, Debug)]
pub struct StoryEvent {
    pub id: u32,
    pub code: [u8; 4],
    pub parameters: Vec<EventParameter>,
}

/// Active quest aliases and the event that supplied them.
#[derive(Clone, Debug)]
pub struct Runtime {
    pub unknown: u8,
    pub aliases: Vec<ReferenceAlias>,
    pub locations: Vec<AliasRef>,
    pub event_flag: u8,
    pub event: Option<StoryEvent>,
}

/// A quest instance's alias text references and snapshot global values.
/// The surrounding scalar meanings are not established by the save schema.
#[derive(Clone, Debug)]
pub struct Instance {
    pub unknown: u32,
    pub aliases: Counted<AliasRef>,
    pub globals: Counted<GlobalVariable>,
    pub unknown_short: u16,
    pub unknown_byte: u8,
}

/// Quest instance section, present independently of active runtime data.
#[derive(Clone, Debug)]
pub struct Instances {
    pub unknown: u32,
    pub instances: Counted<Instance>,
}

/// Fully decoded QUST payload. Absence means the header bit was unset, which
/// is distinct from a present empty array. No part of this type applies state
/// or starts quests, fills aliases, runs fragments or resumes story events.
#[derive(Clone, Debug)]
pub struct QuestChange {
    pub change_flags: u32,
    pub form_flags: Option<FormFlags>,
    pub quest_flags: Option<u16>,
    pub script_delay: Option<f32>,
    pub stages: Option<Counted<QuestStage>>,
    pub objectives: Option<Counted<Objective>>,
    pub runtime: Option<Runtime>,
    pub instances: Option<Instances>,
    pub already_run: Option<u8>,
}

impl ChangeForm {
    /// Decode all documented QUST sections and require exact consumption.
    /// Other change types return None without inflating their payload.
    ///
    /// # Errors
    /// Decompression failures, unknown flags or event tags, impossible counts,
    /// truncated fields or trailing bytes.
    pub fn quest_change(&self) -> Result<Option<QuestChange>> {
        if self.record_type() != "QUST" {
            return Ok(None);
        }
        QuestChange::read(self.change_flags, &self.payload()?).map(Some)
    }
}

impl QuestChange {
    /// Read a decompressed QUST payload under its change-form header flags.
    ///
    /// # Errors
    /// Unsupported flag bits, malformed fields, counts or leftover bytes.
    pub fn read(change_flags: u32, payload: &[u8]) -> Result<Self> {
        if change_flags & !flags::KNOWN != 0 {
            return Err(bad(format!("unknown header flags {change_flags:#010x}")));
        }
        let mut r = Cursor::new(payload, "quest change");
        let form_flags = optional(change_flags, flags::FORM, || {
            Ok(FormFlags {
                flags: r.u32()?,
                unknown: r.u16()?,
            })
        })?;
        let quest_flags = optional(change_flags, flags::QUEST, || r.u16())?;
        let script_delay = optional(change_flags, flags::SCRIPT_DELAY, || r.f32())?;
        let stages = optional(change_flags, flags::STAGES, || {
            counted(&mut r, 3, |r| {
                Ok(QuestStage {
                    stage: r.i16()?,
                    status: r.u8()?,
                })
            })
        })?;
        let objectives = optional(change_flags, flags::OBJECTIVES, || {
            counted(&mut r, 8, |r| {
                Ok(Objective {
                    id: r.u32()?,
                    status: r.u32()?,
                })
            })
        })?;
        let runtime = optional(change_flags, flags::RUNTIME, || read_runtime(&mut r))?;
        let instances = optional(change_flags, flags::INSTANCES, || {
            Ok(Instances {
                unknown: r.u32()?,
                instances: counted(&mut r, 9, read_instance)?,
            })
        })?;
        let already_run = optional(change_flags, flags::ALREADY_RUN, || r.u8())?;
        if !r.is_empty() {
            return Err(bad(format!("{} trailing bytes", r.remaining())));
        }
        Ok(Self {
            change_flags,
            form_flags,
            quest_flags,
            script_delay,
            stages,
            objectives,
            runtime,
            instances,
            already_run,
        })
    }

    /// Re-encode typed fields for comparison with the original payload.
    /// Count widths and float bits are retained. This does not replace the
    /// raw change-form payload used by Save::write.
    ///
    /// # Errors
    /// Changed array sizes, inconsistent optional fields or unsupported tags.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if self.change_flags & !flags::KNOWN != 0 {
            return Err(bad("unknown header flags"));
        }
        for (bit, present) in [
            (flags::FORM, self.form_flags.is_some()),
            (flags::QUEST, self.quest_flags.is_some()),
            (flags::SCRIPT_DELAY, self.script_delay.is_some()),
            (flags::STAGES, self.stages.is_some()),
            (flags::OBJECTIVES, self.objectives.is_some()),
            (flags::RUNTIME, self.runtime.is_some()),
            (flags::INSTANCES, self.instances.is_some()),
            (flags::ALREADY_RUN, self.already_run.is_some()),
        ] {
            if (self.change_flags & bit != 0) != present {
                return Err(bad("header/payload presence mismatch"));
            }
        }
        if let Some(value) = self.form_flags {
            out.extend_from_slice(&value.flags.to_le_bytes());
            out.extend_from_slice(&value.unknown.to_le_bytes());
        }
        if let Some(value) = self.quest_flags {
            out.extend_from_slice(&value.to_le_bytes());
        }
        if let Some(value) = self.script_delay {
            out.extend_from_slice(&value.to_le_bytes());
        }
        if let Some(stages) = &self.stages {
            write_counted(stages, &mut out, |stage, out| {
                out.extend_from_slice(&stage.stage.to_le_bytes());
                out.push(stage.status);
                Ok(())
            })?;
        }
        if let Some(objectives) = &self.objectives {
            write_counted(objectives, &mut out, |objective, out| {
                out.extend_from_slice(&objective.id.to_le_bytes());
                out.extend_from_slice(&objective.status.to_le_bytes());
                Ok(())
            })?;
        }
        if let Some(runtime) = &self.runtime {
            write_runtime(runtime, &mut out)?;
        }
        if let Some(instances) = &self.instances {
            out.extend_from_slice(&instances.unknown.to_le_bytes());
            write_counted(&instances.instances, &mut out, |instance, out| {
                out.extend_from_slice(&instance.unknown.to_le_bytes());
                write_counted(&instance.aliases, out, |alias, out| {
                    write_alias(*alias, out);
                    Ok(())
                })?;
                write_counted(&instance.globals, out, |global, out| {
                    global.form.write(out);
                    out.extend_from_slice(&global.value.to_le_bytes());
                    Ok(())
                })?;
                out.extend_from_slice(&instance.unknown_short.to_le_bytes());
                out.push(instance.unknown_byte);
                Ok(())
            })?;
        }
        if let Some(value) = self.already_run {
            out.push(value);
        }
        Ok(out)
    }
}

fn read_runtime(r: &mut Cursor<'_>) -> Result<Runtime> {
    let unknown = r.u8()?;
    let aliases = fixed_count(r, 8, |r| {
        let id = r.u32()?;
        let kind = r.u8()?;
        let references = (0..if kind == 0 { 1 } else { 5 })
            .map(|_| r.ref_id())
            .collect::<Result<Vec<_>>>()?;
        Ok(ReferenceAlias {
            id,
            kind,
            references,
        })
    })?;
    let locations = fixed_count(r, 7, read_alias)?;
    let event_flag = r.u8()?;
    let event = if event_flag == 0 {
        None
    } else {
        Some(read_event(r)?)
    };
    Ok(Runtime {
        unknown,
        aliases,
        locations,
        event_flag,
        event,
    })
}

fn read_event(r: &mut Cursor<'_>) -> Result<StoryEvent> {
    let id = r.u32()?;
    let bytes = r.take(4)?;
    let code = [bytes[0], bytes[1], bytes[2], bytes[3]];
    let parameters = fixed_count(r, 7, |r| match r.u32()? {
        kind @ (0..=2 | 4) => Ok(EventParameter::Reference {
            kind,
            reference: r.ref_id()?,
        }),
        3 => Ok(EventParameter::Integer(r.u32()?)),
        other => Err(bad(format!("unknown event parameter type {other}"))),
    })?;
    Ok(StoryEvent {
        id,
        code,
        parameters,
    })
}

fn read_instance(r: &mut Cursor<'_>) -> Result<Instance> {
    Ok(Instance {
        unknown: r.u32()?,
        aliases: counted(r, 7, read_alias)?,
        globals: counted(r, 7, |r| {
            Ok(GlobalVariable {
                form: r.ref_id()?,
                value: r.f32()?,
            })
        })?,
        unknown_short: r.u16()?,
        unknown_byte: r.u8()?,
    })
}

fn read_alias(r: &mut Cursor<'_>) -> Result<AliasRef> {
    Ok(AliasRef {
        id: r.u32()?,
        reference: r.ref_id()?,
    })
}

fn optional<T>(flags: u32, bit: u32, read: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    if flags & bit == 0 {
        Ok(None)
    } else {
        read().map(Some)
    }
}

fn counted<T>(
    r: &mut Cursor<'_>,
    size: usize,
    read: impl FnMut(&mut Cursor<'_>) -> Result<T>,
) -> Result<Counted<T>> {
    let count = r.vsval()?;
    let entries = elements(r, count.value(), size, read)?;
    Ok(Counted { count, entries })
}

fn fixed_count<T>(
    r: &mut Cursor<'_>,
    size: usize,
    read: impl FnMut(&mut Cursor<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    let count = r.u32()?;
    elements(r, count, size, read)
}

fn elements<T>(
    r: &mut Cursor<'_>,
    count: u32,
    size: usize,
    mut read: impl FnMut(&mut Cursor<'_>) -> Result<T>,
) -> Result<Vec<T>> {
    if count as usize > r.remaining() / size {
        return Err(bad(format!("count {count} exceeds remaining payload")));
    }
    (0..count).map(|_| read(r)).collect()
}

fn write_counted<T>(
    values: &Counted<T>,
    out: &mut Vec<u8>,
    mut write: impl FnMut(&T, &mut Vec<u8>) -> Result<()>,
) -> Result<()> {
    if values.count.value() as usize != values.entries.len() {
        return Err(bad("count changed after decode"));
    }
    values.count.write(out);
    for value in &values.entries {
        write(value, out)?;
    }
    Ok(())
}

fn write_len(len: usize, out: &mut Vec<u8>) -> Result<()> {
    let len = u32::try_from(len).map_err(|_| bad("array too large"))?;
    out.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

fn write_alias(alias: AliasRef, out: &mut Vec<u8>) {
    out.extend_from_slice(&alias.id.to_le_bytes());
    alias.reference.write(out);
}

fn write_runtime(runtime: &Runtime, out: &mut Vec<u8>) -> Result<()> {
    if (runtime.event_flag != 0) != runtime.event.is_some() {
        return Err(bad("event flag/payload mismatch"));
    }
    out.push(runtime.unknown);
    write_len(runtime.aliases.len(), out)?;
    for alias in &runtime.aliases {
        if alias.references.len() != if alias.kind == 0 { 1 } else { 5 } {
            return Err(bad("alias kind/reference count mismatch"));
        }
        out.extend_from_slice(&alias.id.to_le_bytes());
        out.push(alias.kind);
        for reference in &alias.references {
            reference.write(out);
        }
    }
    write_len(runtime.locations.len(), out)?;
    for alias in &runtime.locations {
        write_alias(*alias, out);
    }
    out.push(runtime.event_flag);
    if let Some(event) = &runtime.event {
        write_event(event, out)?;
    }
    Ok(())
}

fn write_event(event: &StoryEvent, out: &mut Vec<u8>) -> Result<()> {
    out.extend_from_slice(&event.id.to_le_bytes());
    out.extend_from_slice(&event.code);
    write_len(event.parameters.len(), out)?;
    for parameter in &event.parameters {
        match parameter {
            EventParameter::Reference {
                kind: kind @ (0..=2 | 4),
                reference,
            } => {
                out.extend_from_slice(&kind.to_le_bytes());
                reference.write(out);
            }
            EventParameter::Integer(value) => {
                out.extend_from_slice(&3_u32.to_le_bytes());
                out.extend_from_slice(&value.to_le_bytes());
            }
            EventParameter::Reference { kind, .. } => {
                return Err(bad(format!("unknown event parameter type {kind}")))
            }
        }
    }
    Ok(())
}

fn bad(reason: impl Into<String>) -> Error {
    Error::Quest {
        reason: reason.into(),
    }
}
