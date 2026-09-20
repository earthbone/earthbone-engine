// SPDX-License-Identifier: GPL-3.0-or-later

//! The global variable table, global data type 3.
//!
//! A count then one reference and one float each. These are the same globals
//! `GlobalVariable.GetValue` reads in game, so they map straight onto the
//! harness `expect global` check.

use crate::cursor::{Cursor, RefId, VsVal};
use crate::error::Result;

#[derive(Clone, Copy, Debug)]
pub struct GlobalVariable {
    pub form: RefId,
    pub value: f32,
}

#[derive(Clone, Debug, Default)]
pub struct GlobalVariables {
    pub variables: Vec<GlobalVariable>,
}

impl GlobalVariables {
    pub fn read(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data, "global variables");
        let count = cursor.vsval()?.value();
        if cursor.remaining() != count as usize * 7 {
            return Err(crate::error::Error::GlobalVariableSize {
                count,
                bytes: cursor.remaining(),
            });
        }
        let mut variables = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let form = cursor.ref_id()?;
            let value = cursor.f32()?;
            variables.push(GlobalVariable { form, value });
        }
        Ok(GlobalVariables { variables })
    }

    pub fn write(&self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        VsVal::from_value(u32::try_from(self.variables.len()).ok()?)?.write(&mut out);
        for variable in &self.variables {
            variable.form.write(&mut out);
            out.extend_from_slice(&variable.value.to_le_bytes());
        }
        Some(out)
    }
}
