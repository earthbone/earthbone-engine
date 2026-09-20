// SPDX-License-Identifier: GPL-3.0-or-later

//! Execution tables following array values in a Skyrim Papyrus save block.
//! Layouts follow FallrimTools ActiveScript, FunctionMessage,
//! FunctionMessageData, SuspendedStack, QueuedUnbind and Papyrus.java.

use std::collections::BTreeSet;

use super::{
    bad, frames::write_count, read_count, read_string, unique, values, Cursor, FragmentTask,
    Result, StackFrame, StringRef, Value,
};

/// Decoded execution state. Reading this does not schedule saved threads.
#[derive(Clone, Debug)]
pub struct Execution {
    pub active_data: Vec<ActiveScriptData>,
    pub messages: Vec<FunctionMessage>,
    pub suspended: [Vec<SuspendedStack>; 2],
    pub unknown: u32,
    pub extra: Option<u32>,
    pub unknown_ids: Vec<u64>,
    pub unbinds: Vec<QueuedUnbind>,
    pub save_file_version: u16,
}

/// A thread body linked by ID to a first-pass active-script header.
#[derive(Clone, Debug)]
pub struct ActiveScriptData {
    pub id: u32,
    pub version: [u8; 2],
    pub unknown: Value,
    pub flags: u8,
    pub unknown_byte: u8,
    pub extra: Option<u32>,
    pub fragment_kind: u8,
    pub fragment: Option<FragmentTask>,
    pub frames: Vec<StackFrame>,
    pub trailing: Option<u8>,
}

/// A queued call with its optional thread ID and message payload.
#[derive(Clone, Debug)]
pub struct FunctionMessage {
    pub flag: i8,
    pub id: Option<u32>,
    pub flags: u8,
    pub message: Option<FunctionMessageData>,
}

/// Shared payload for queued function messages and suspended stacks.
#[derive(Clone, Debug)]
pub struct FunctionMessageData {
    pub flags: u8,
    pub script_name: StringRef,
    pub event: StringRef,
    pub unknown: Value,
    pub variables: Vec<Value>,
}

/// One entry in either suspended-stack table. Unresolved IDs are retained.
#[derive(Clone, Debug)]
pub struct SuspendedStack {
    pub id: u32,
    pub flags: u8,
    pub message: Option<FunctionMessageData>,
}

/// A pending script-instance unbind with its unknown 32-bit payload.
#[derive(Clone, Debug)]
pub struct QueuedUnbind {
    pub id: u64,
    pub unknown: u32,
}

impl Execution {
    pub(super) fn read(
        cursor: &mut Cursor<'_>,
        strings: usize,
        mut ids: BTreeSet<u64>,
    ) -> Result<Self> {
        let mut active_data = Vec::with_capacity(ids.len());
        for _ in 0..ids.len() {
            let id = cursor.u32()?;
            if !ids.remove(&u64::from(id)) {
                return Err(bad(format!("unknown or repeated active data ID {id:#x}")));
            }
            active_data.push(ActiveScriptData::read(cursor, strings, id)?);
        }
        let count = read_count(cursor, 2, "function messages")?;
        let mut messages = Vec::with_capacity(count);
        for _ in 0..count {
            let flag = cursor.u8()? as i8;
            let id = if flag <= 2 { Some(cursor.u32()?) } else { None };
            let flags = cursor.u8()?;
            let message = FunctionMessageData::read_optional(cursor, strings, flags)?;
            messages.push(FunctionMessage {
                flag,
                id,
                flags,
                message,
            });
        }
        let suspended = [
            read_suspended(cursor, strings)?,
            read_suspended(cursor, strings)?,
        ];
        let unknown = cursor.u32()?;
        let extra = if unknown != 0 {
            Some(cursor.u32()?)
        } else {
            None
        };
        let count = read_count(cursor, 8, "unknown execution IDs")?;
        let mut unknown_ids = Vec::with_capacity(count);
        for _ in 0..count {
            unknown_ids.push(cursor.u64()?);
        }
        let count = read_count(cursor, 12, "queued unbinds")?;
        let mut unbinds = Vec::with_capacity(count);
        for _ in 0..count {
            unbinds.push(QueuedUnbind {
                id: cursor.u64()?,
                unknown: cursor.u32()?,
            });
        }
        Ok(Self {
            active_data,
            messages,
            suspended,
            unknown,
            extra,
            unknown_ids,
            unbinds,
            save_file_version: cursor.u16()?,
        })
    }

    /// Re-encode decoded execution tables for comparison with the source slice.
    /// Returns an error for inconsistent optional fields or count overflows.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for active in &self.active_data {
            active.write(&mut out)?;
        }
        write_count(&mut out, self.messages.len())?;
        for message in &self.messages {
            if message.id.is_some() != (message.flag <= 2) {
                return Err(bad("function message ID disagrees with flag"));
            }
            out.push(message.flag as u8);
            if let Some(id) = message.id {
                out.extend_from_slice(&id.to_le_bytes());
            }
            out.push(message.flags);
            FunctionMessageData::write_optional(&mut out, message.flags, message.message.as_ref())?;
        }
        for table in &self.suspended {
            write_count(&mut out, table.len())?;
            for stack in table {
                out.extend_from_slice(&stack.id.to_le_bytes());
                out.push(stack.flags);
                FunctionMessageData::write_optional(&mut out, stack.flags, stack.message.as_ref())?;
            }
        }
        if self.extra.is_some() != (self.unknown != 0) {
            return Err(bad("execution extra disagrees with unknown field"));
        }
        out.extend_from_slice(&self.unknown.to_le_bytes());
        if let Some(extra) = self.extra {
            out.extend_from_slice(&extra.to_le_bytes());
        }
        write_count(&mut out, self.unknown_ids.len())?;
        for id in &self.unknown_ids {
            out.extend_from_slice(&id.to_le_bytes());
        }
        write_count(&mut out, self.unbinds.len())?;
        for unbind in &self.unbinds {
            out.extend_from_slice(&unbind.id.to_le_bytes());
            out.extend_from_slice(&unbind.unknown.to_le_bytes());
        }
        out.extend_from_slice(&self.save_file_version.to_le_bytes());
        Ok(out)
    }
}

impl ActiveScriptData {
    fn read(cursor: &mut Cursor<'_>, strings: usize, id: u32) -> Result<Self> {
        let version = [cursor.u8()?, cursor.u8()?];
        if version[0] > 0 && !matches!(version[1], 1 | 2) {
            return Err(bad("unsupported active script version"));
        }
        let unknown = Value::read(cursor, strings, 0)?;
        let flags = cursor.u8()?;
        let unknown_byte = cursor.u8()?;
        let extra = if flags & 1 != 0 {
            Some(cursor.u32()?)
        } else {
            None
        };
        let fragment_kind = cursor.u8()?;
        let fragment = if matches!(fragment_kind, 1..=3) {
            Some(FragmentTask::read(cursor, strings, fragment_kind)?)
        } else {
            None
        };
        let count = read_count(cursor, 44, "stack frames")?;
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            frames.push(StackFrame::read(cursor, strings)?);
        }
        let trailing = if frames.is_empty() {
            None
        } else {
            Some(cursor.u8()?)
        };
        Ok(Self {
            id,
            version,
            unknown,
            flags,
            unknown_byte,
            extra,
            fragment_kind,
            fragment,
            frames,
            trailing,
        })
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.extra.is_some() != (self.flags & 1 != 0)
            || self.fragment.is_some() != matches!(self.fragment_kind, 1..=3)
            || self.trailing.is_some() == self.frames.is_empty()
            || self.version[0] > 0 && !matches!(self.version[1], 1 | 2)
        {
            return Err(bad("inconsistent active script fields"));
        }
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.version);
        self.unknown.write(out)?;
        out.extend_from_slice(&[self.flags, self.unknown_byte]);
        if let Some(extra) = self.extra {
            out.extend_from_slice(&extra.to_le_bytes());
        }
        out.push(self.fragment_kind);
        if let Some(fragment) = &self.fragment {
            fragment.write(out, self.fragment_kind)?;
        }
        write_count(out, self.frames.len())?;
        for frame in &self.frames {
            frame.write(out)?;
        }
        if let Some(trailing) = self.trailing {
            out.push(trailing);
        }
        Ok(())
    }
}

impl FunctionMessageData {
    fn read_optional(cursor: &mut Cursor<'_>, strings: usize, flags: u8) -> Result<Option<Self>> {
        if flags == 0 {
            return Ok(None);
        }
        let flags = cursor.u8()?;
        let script_name = read_string(cursor, strings)?;
        let event = read_string(cursor, strings)?;
        let unknown = Value::read(cursor, strings, 0)?;
        let count = read_count(cursor, 5, "message variables")?;
        let variables = values::read_values(cursor, strings, count)?;
        Ok(Some(Self {
            flags,
            script_name,
            event,
            unknown,
            variables,
        }))
    }

    fn write_optional(out: &mut Vec<u8>, flags: u8, message: Option<&Self>) -> Result<()> {
        if message.is_some() != (flags != 0) {
            return Err(bad("message payload disagrees with flags"));
        }
        if let Some(message) = message {
            out.push(message.flags);
            out.extend_from_slice(&message.script_name.0.to_le_bytes());
            out.extend_from_slice(&message.event.0.to_le_bytes());
            message.unknown.write(out)?;
            write_count(out, message.variables.len())?;
            for value in &message.variables {
                value.write(out)?;
            }
        }
        Ok(())
    }
}

fn read_suspended(cursor: &mut Cursor<'_>, strings: usize) -> Result<Vec<SuspendedStack>> {
    let count = read_count(cursor, 5, "suspended stacks")?;
    let mut stacks = Vec::with_capacity(count);
    let mut ids = BTreeSet::new();
    for _ in 0..count {
        let id = cursor.u32()?;
        unique(&mut ids, u64::from(id), "suspended stack")?;
        let flags = cursor.u8()?;
        let message = FunctionMessageData::read_optional(cursor, strings, flags)?;
        stacks.push(SuspendedStack { id, flags, message });
    }
    Ok(stacks)
}
