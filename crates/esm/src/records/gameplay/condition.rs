// SPDX-License-Identifier: GPL-3.0-or-later

//! CTDA and its optional CIS1/CIS2 strings, from xEdit wbConditions.
//! Function-specific parameter interpretation and evaluation remain with the caller.

use super::{fields, Result};
use crate::cursor::zstring;
use crate::{Error, Record, Subrecord};

/// A numeric comparison or a local GLOB FormID, selected by the CTDA flags.
#[derive(Debug, Clone, PartialEq)]
pub enum Comparison {
    Number(f32),
    Global(u32),
}

/// One condition in file order. Parameter unions are kept as raw bits.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// Comparison operator in the high three bits, condition flags below.
    pub flags: u8,
    pub comparison: Comparison,
    pub function: u16,
    /// Raw unions, not necessarily FormIDs.
    pub parameters: [u32; 2],
    pub run_on: u32,
    /// A FormID only when the run-on mode calls for a reference.
    pub reference: u32,
    pub parameter_3: i32,
    pub strings: [Option<String>; 2],
}

impl Condition {
    /// The raw comparison operator, without interpreting unknown values.
    pub fn operator(&self) -> u8 {
        self.flags >> 5
    }
    /// Whether the OR flag joins this condition to the next one.
    pub fn or_with_next(&self) -> bool {
        self.flags & 1 != 0
    }

    pub(super) fn read(rec: &Record, sub: &Subrecord) -> Result<Self> {
        let mut f = fields(rec, sub.tag, &sub.data, 32)?;
        let flags = f.u8()?;
        f.skip(3)?;
        let bits = f.u32()?;
        let comparison = if flags & 4 != 0 {
            Comparison::Global(bits)
        } else {
            Comparison::Number(f32::from_bits(bits))
        };
        let function = f.u16()?;
        f.skip(2)?;
        Ok(Self {
            flags,
            comparison,
            function,
            parameters: [f.u32()?, f.u32()?],
            run_on: f.u32()?,
            reference: f.u32()?,
            parameter_3: f.i32()?,
            strings: [None, None],
        })
    }

    /// Returns true only for condition subrecords.
    ///
    /// # Errors
    ///
    /// Rejects malformed CTDA data and orphaned or repeated condition strings.
    pub fn push(rec: &Record, sub: &Subrecord, out: &mut Vec<Self>) -> Result<bool> {
        match sub.tag.as_bytes() {
            b"CTDA" => out.push(Self::read(rec, sub)?),
            b"CIS1" | b"CIS2" => {
                let condition = out.last_mut().ok_or_else(|| {
                    Error::Invalid(format!("{} {} without CTDA", rec.tag, sub.tag))
                })?;
                let index = usize::from(sub.tag.as_bytes() == b"CIS2");
                if condition.strings[index].is_some() {
                    return Err(Error::Invalid(format!("repeated {} in condition", sub.tag)));
                }
                condition.strings[index] = Some(zstring(&sub.data));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn all(rec: &Record) -> Result<Vec<Self>> {
        let mut out = Vec::new();
        for sub in &rec.subrecords {
            Self::push(rec, sub, &mut out)?;
        }
        Ok(out)
    }
}
