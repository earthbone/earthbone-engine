// SPDX-License-Identifier: GPL-3.0-or-later

//! Papyrus value layouts from FallrimTools Variable.java and Type.java.
//! Element IDs are eight bytes in SE, including array and object references.

use super::{bad, read_string, StringRef};
use crate::cursor::Cursor;
use crate::error::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueType {
    Null = 0,
    Reference = 1,
    String = 2,
    Integer = 3,
    Float = 4,
    Boolean = 5,
    Variant = 6,
    Struct = 7,
}

impl ValueType {
    pub(super) fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        match cursor.u8()? {
            0 => Ok(Self::Null),
            1 => Ok(Self::Reference),
            2 => Ok(Self::String),
            3 => Ok(Self::Integer),
            4 => Ok(Self::Float),
            5 => Ok(Self::Boolean),
            6 => Ok(Self::Variant),
            7 => Ok(Self::Struct),
            other => Err(bad(format!("invalid array element type {other}"))),
        }
    }

    pub(super) fn has_type_name(self) -> bool {
        matches!(self, Self::Reference | Self::Struct)
    }
}

/// Values retain raw null and boolean payloads and unresolved element IDs.
/// A missing referent does not make an otherwise readable save malformed.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null(u32),
    Reference {
        type_name: StringRef,
        id: u64,
    },
    String(StringRef),
    Integer(i32),
    Float(f32),
    Boolean(u32),
    Variant(Box<Value>),
    Struct {
        type_name: StringRef,
        id: u64,
    },
    Array {
        element_type: ValueType,
        type_name: Option<StringRef>,
        id: u64,
    },
}

impl Value {
    pub(super) fn read(cursor: &mut Cursor<'_>, strings: usize, depth: usize) -> Result<Self> {
        if depth >= 128 {
            return Err(bad("variant nesting exceeds 128"));
        }
        Ok(match cursor.u8()? {
            0 => Self::Null(cursor.u32()?),
            1 => Self::Reference {
                type_name: read_string(cursor, strings)?,
                id: cursor.u64()?,
            },
            2 => Self::String(read_string(cursor, strings)?),
            3 => Self::Integer(cursor.u32()? as i32),
            4 => Self::Float(cursor.f32()?),
            5 => Self::Boolean(cursor.u32()?),
            6 => Self::Variant(Box::new(Self::read(cursor, strings, depth + 1)?)),
            7 => Self::Struct {
                type_name: read_string(cursor, strings)?,
                id: cursor.u64()?,
            },
            tag @ 11..=17 => {
                let element_type = ValueType::read(&mut Cursor::new(&[tag - 10], "value type"))?;
                let type_name = if element_type.has_type_name() {
                    Some(read_string(cursor, strings)?)
                } else {
                    None
                };
                Self::Array {
                    element_type,
                    type_name,
                    id: cursor.u64()?,
                }
            }
            tag => return Err(bad(format!("invalid variable type {tag}"))),
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        match self {
            Self::Null(v) => {
                out.push(0);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Reference { type_name, id } | Self::Struct { type_name, id } => {
                out.push(if matches!(self, Self::Reference { .. }) {
                    1
                } else {
                    7
                });
                out.extend_from_slice(&type_name.0.to_le_bytes());
                out.extend_from_slice(&id.to_le_bytes());
            }
            Self::String(v) => {
                out.push(2);
                out.extend_from_slice(&v.0.to_le_bytes());
            }
            Self::Integer(v) => {
                out.push(3);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Float(v) => {
                out.push(4);
                out.extend_from_slice(&v.to_bits().to_le_bytes());
            }
            Self::Boolean(v) => {
                out.push(5);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Self::Variant(v) => {
                out.push(6);
                v.write(out)?;
            }
            Self::Array {
                element_type,
                type_name,
                id,
            } => {
                if *element_type == ValueType::Null
                    || element_type.has_type_name() != type_name.is_some()
                {
                    return Err(bad("invalid array value type/name"));
                }
                out.push(*element_type as u8 + 10);
                if let Some(name) = type_name {
                    out.extend_from_slice(&name.0.to_le_bytes());
                }
                out.extend_from_slice(&id.to_le_bytes());
            }
        }
        Ok(())
    }
}

pub(super) fn read_values(
    cursor: &mut Cursor<'_>,
    strings: usize,
    count: usize,
) -> Result<Vec<Value>> {
    if count > cursor.remaining() / 5 {
        return Err(bad(format!("{count} values exceed the remaining payload")));
    }
    (0..count)
        .map(|_| Value::read(cursor, strings, 0))
        .collect()
}
