// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved instructions, following FallrimTools OpcodeData and Parameter.
//! The opcode table is shared with the PEX reader. Saved operands use 32-bit
//! string indices and byte booleans, unlike runtime variable payloads.

use soe_papyrus::pex::Opcode;

use super::{bad, read_string, Cursor, Result, StringRef};

/// An operand embedded in a saved stack frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Null,
    Identifier(StringRef),
    String(StringRef),
    Integer(i32),
    Float(f32),
    Boolean(u8),
    Unknown8(StringRef),
}

/// Fixed operands followed by the encoded argument count and call arguments.
#[derive(Clone, Debug)]
pub struct Instruction {
    pub opcode: Opcode,
    pub operands: Vec<Operand>,
}

impl Operand {
    fn read(cursor: &mut Cursor<'_>, strings: usize) -> Result<Self> {
        Ok(match cursor.u8()? {
            0 => Self::Null,
            1 => Self::Identifier(read_string(cursor, strings)?),
            2 => Self::String(read_string(cursor, strings)?),
            3 => Self::Integer(cursor.u32()? as i32),
            4 => Self::Float(cursor.f32()?),
            5 => Self::Boolean(cursor.u8()?),
            8 => Self::Unknown8(read_string(cursor, strings)?),
            tag => return Err(bad(format!("invalid saved instruction operand tag {tag}"))),
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Null => out.push(0),
            Self::Identifier(value) | Self::String(value) | Self::Unknown8(value) => {
                out.push(match self {
                    Self::Identifier(_) => 1,
                    Self::String(_) => 2,
                    _ => 8,
                });
                out.extend_from_slice(&value.0.to_le_bytes());
            }
            Self::Integer(value) => {
                out.push(3);
                out.extend_from_slice(&value.to_le_bytes());
            }
            Self::Float(value) => {
                out.push(4);
                out.extend_from_slice(&value.to_le_bytes());
            }
            Self::Boolean(value) => out.extend_from_slice(&[5, *value]),
        }
    }
}

impl Instruction {
    pub(super) fn read(cursor: &mut Cursor<'_>, strings: usize) -> Result<Self> {
        let code = cursor.u8()?;
        let opcode = Opcode::from_u8(code)
            .filter(|opcode| !opcode.is_fallout4())
            .ok_or_else(|| bad(format!("unsupported Skyrim saved opcode {code}")))?;
        let (fixed, variable) = opcode.arity();
        let mut operands = Vec::with_capacity(fixed + usize::from(variable));
        for _ in 0..fixed {
            operands.push(Operand::read(cursor, strings)?);
        }
        if variable {
            let count = Operand::read(cursor, strings)?;
            let Operand::Integer(n) = count else {
                return Err(bad("saved call argument count is not an integer"));
            };
            if n < 0 || n as usize > cursor.remaining() {
                return Err(bad("saved call argument count exceeds payload"));
            }
            operands.push(count);
            for _ in 0..n {
                operands.push(Operand::read(cursor, strings)?);
            }
        }
        Ok(Self { opcode, operands })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        let (fixed, variable) = self.opcode.arity();
        let expected = if variable {
            let Some(Operand::Integer(n)) = self.operands.get(fixed) else {
                return Err(bad("saved call argument count is not an integer"));
            };
            fixed + 1 + usize::try_from(*n).map_err(|_| bad("negative call argument count"))?
        } else {
            fixed
        };
        if self.opcode.is_fallout4() || self.operands.len() != expected {
            return Err(bad("invalid Skyrim saved instruction operands"));
        }
        out.push(self.opcode as u8);
        for operand in &self.operands {
            operand.write(out);
        }
        Ok(())
    }
}
