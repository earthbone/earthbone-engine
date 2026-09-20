// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved Papyrus frames, following FallrimTools StackFrame.java.

use super::{
    bad, read_count, read_string, values, Cursor, Instruction, MemberDesc, Result, StringRef, Value,
};

/// A saved function body and its execution position. IDs remain unresolved.
#[derive(Clone, Debug)]
pub struct StackFrame {
    pub flags: u8,
    pub function_type: u8,
    pub script_name: StringRef,
    pub base_name: StringRef,
    pub event: StringRef,
    pub status: Option<StringRef>,
    pub opcode_version: [u8; 2],
    pub return_type: StringRef,
    pub docstring: StringRef,
    pub user_flags: u32,
    pub function_flags: u8,
    pub parameters: Vec<MemberDesc>,
    pub locals: Vec<MemberDesc>,
    pub instructions: Vec<Instruction>,
    pub pointer: u32,
    pub owner: Value,
    pub variables: Vec<Value>,
}

impl StackFrame {
    pub(super) fn read(cursor: &mut Cursor<'_>, strings: usize) -> Result<Self> {
        let count = read_count(cursor, 5, "frame variables")?;
        let flags = cursor.u8()?;
        let function_type = cursor.u8()?;
        if !matches!(function_type, 0..=7 | 11..=17) {
            return Err(bad(format!("invalid frame function type {function_type}")));
        }
        let script_name = read_string(cursor, strings)?;
        let base_name = read_string(cursor, strings)?;
        let event = read_string(cursor, strings)?;
        let status = if flags & 1 == 0 && function_type == 0 {
            Some(read_string(cursor, strings)?)
        } else {
            None
        };
        let opcode_version = [cursor.u8()?, cursor.u8()?];
        let return_type = read_string(cursor, strings)?;
        let docstring = read_string(cursor, strings)?;
        let user_flags = cursor.u32()?;
        let function_flags = cursor.u8()?;
        let parameters = read_members(cursor, strings)?;
        let locals = read_members(cursor, strings)?;
        let code_count = cursor.u16()? as usize;
        if code_count > cursor.remaining() {
            return Err(bad("instruction count exceeds frame payload"));
        }
        let mut instructions = Vec::with_capacity(code_count);
        for _ in 0..code_count {
            instructions.push(Instruction::read(cursor, strings)?);
        }
        let pointer = cursor.u32()?;
        let owner = Value::read(cursor, strings, 0)?;
        let variables = values::read_values(cursor, strings, count)?;
        Ok(Self {
            flags,
            function_type,
            script_name,
            base_name,
            event,
            status,
            opcode_version,
            return_type,
            docstring,
            user_flags,
            function_flags,
            parameters,
            locals,
            instructions,
            pointer,
            owner,
            variables,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if !matches!(self.function_type, 0..=7 | 11..=17)
            || self.status.is_some() != (self.flags & 1 == 0 && self.function_type == 0)
        {
            return Err(bad("invalid frame function type or status"));
        }
        write_count(out, self.variables.len())?;
        out.extend_from_slice(&[self.flags, self.function_type]);
        for name in [self.script_name, self.base_name, self.event] {
            out.extend_from_slice(&name.0.to_le_bytes());
        }
        if let Some(status) = self.status {
            out.extend_from_slice(&status.0.to_le_bytes());
        }
        out.extend_from_slice(&self.opcode_version);
        out.extend_from_slice(&self.return_type.0.to_le_bytes());
        out.extend_from_slice(&self.docstring.0.to_le_bytes());
        out.extend_from_slice(&self.user_flags.to_le_bytes());
        out.push(self.function_flags);
        write_members(out, &self.parameters)?;
        write_members(out, &self.locals)?;
        write_short_count(out, self.instructions.len())?;
        for instruction in &self.instructions {
            instruction.write(out)?;
        }
        out.extend_from_slice(&self.pointer.to_le_bytes());
        self.owner.write(out)?;
        for value in &self.variables {
            value.write(out)?;
        }
        Ok(())
    }
}

fn read_members(cursor: &mut Cursor<'_>, strings: usize) -> Result<Vec<MemberDesc>> {
    let count = cursor.u16()? as usize;
    if count > cursor.remaining() / 8 {
        return Err(bad("frame member count exceeds payload"));
    }
    let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        members.push(MemberDesc {
            name: read_string(cursor, strings)?,
            type_name: read_string(cursor, strings)?,
        });
    }
    Ok(members)
}

fn write_members(out: &mut Vec<u8>, members: &[MemberDesc]) -> Result<()> {
    write_short_count(out, members.len())?;
    for member in members {
        out.extend_from_slice(&member.name.0.to_le_bytes());
        out.extend_from_slice(&member.type_name.0.to_le_bytes());
    }
    Ok(())
}

pub(super) fn write_count(out: &mut Vec<u8>, count: usize) -> Result<()> {
    out.extend_from_slice(
        &u32::try_from(count)
            .map_err(|_| bad("table exceeds 32-bit count"))?
            .to_le_bytes(),
    );
    Ok(())
}

fn write_short_count(out: &mut Vec<u8>, count: usize) -> Result<()> {
    out.extend_from_slice(
        &u16::try_from(count)
            .map_err(|_| bad("frame table exceeds 16-bit count"))?
            .to_le_bytes(),
    );
    Ok(())
}
