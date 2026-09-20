// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed fields for extra-data structures whose runtime meaning is still partial.
//! Counts keep their original width. A diagnostic writer validates edited lengths.

use crate::cursor::{Cursor, VsVal};
use crate::{Error, RefId, Result, WString};

#[derive(Clone, Debug)]
enum Count {
    Variable(VsVal),
    Fixed(u32),
}

/// A count followed by records in saved order.
#[derive(Clone, Debug)]
pub struct FieldArray {
    count: Count,
    pub entries: Vec<Vec<Field>>,
}

/// A saved scalar or structured field. Float bits are preserved by encoding.
#[derive(Clone, Debug)]
pub enum Field {
    Byte(u8),
    Short(u16),
    Word(u32),
    Float(f32),
    Reference(RefId),
    String(WString),
    Bytes(Vec<u8>),
    Array(FieldArray),
    Group(Vec<Field>),
}

pub(super) fn invalid(reason: impl Into<String>) -> Error {
    Error::Reference {
        reason: reason.into(),
    }
}

pub(super) fn array(
    r: &mut Cursor<'_>,
    fixed: bool,
    minimum: usize,
    mut read: impl FnMut(&mut Cursor<'_>) -> Result<Vec<Field>>,
) -> Result<FieldArray> {
    let count = if fixed {
        Count::Fixed(r.u32()?)
    } else {
        Count::Variable(r.vsval()?)
    };
    let n = match count {
        Count::Fixed(n) => n,
        Count::Variable(n) => n.value(),
    } as usize;
    if n > r.remaining() / minimum.max(1) {
        return Err(invalid("extra-data array exceeds remaining bytes"));
    }
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        entries.push(read(r)?);
    }
    Ok(FieldArray { count, entries })
}

impl FieldArray {
    pub(super) fn variable(count: VsVal, entries: Vec<Vec<Field>>) -> Self {
        Self {
            count: Count::Variable(count),
            entries,
        }
    }

    pub(super) fn fixed(entries: Vec<Vec<Field>>) -> Result<Self> {
        let count =
            u32::try_from(entries.len()).map_err(|_| invalid("extra-data array too long"))?;
        Ok(Self {
            count: Count::Fixed(count),
            entries,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        let n = match self.count {
            Count::Fixed(n) => n,
            Count::Variable(n) => n.value(),
        };
        if n as usize != self.entries.len() {
            return Err(invalid("extra-data count mismatch"));
        }
        match self.count {
            Count::Fixed(n) => out.extend(n.to_le_bytes()),
            Count::Variable(n) => n.write(out),
        }
        for fields in &self.entries {
            write(fields, out)?;
        }
        Ok(())
    }
}

pub(super) fn write(fields: &[Field], out: &mut Vec<u8>) -> Result<()> {
    for field in fields {
        match field {
            Field::Byte(v) => out.push(*v),
            Field::Short(v) => out.extend(v.to_le_bytes()),
            Field::Word(v) => out.extend(v.to_le_bytes()),
            Field::Float(v) => out.extend(v.to_le_bytes()),
            Field::Reference(v) => v.write(out),
            Field::String(v) => v.write(out),
            Field::Bytes(v) => out.extend(v),
            Field::Array(v) => v.write(out)?,
            Field::Group(v) => write(v, out)?,
        }
    }
    Ok(())
}

/// Read a fixed field sequence. The alphabet names widths, not runtime semantics.
pub(super) fn fields(r: &mut Cursor<'_>, schema: &str) -> Result<Vec<Field>> {
    schema
        .chars()
        .map(|kind| {
            Ok(match kind {
                'b' => Field::Byte(r.u8()?),
                's' => Field::Short(r.u16()?),
                'w' => Field::Word(r.u32()?),
                'f' => Field::Float(r.f32()?),
                'r' => Field::Reference(r.ref_id()?),
                't' => Field::String(r.wstring()?),
                _ => return Err(invalid("invalid extra-data field schema")),
            })
        })
        .collect()
}

pub(super) fn list(r: &mut Cursor<'_>, schema: &str) -> Result<Field> {
    let minimum = schema
        .chars()
        .map(|kind| match kind {
            'b' => 1,
            's' | 't' => 2,
            'r' => 3,
            _ => 4,
        })
        .sum();
    Ok(Field::Array(array(r, false, minimum, |r| {
        fields(r, schema)
    })?))
}
