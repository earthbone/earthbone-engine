// SPDX-License-Identifier: GPL-3.0-or-later

//! The Papyrus virtual machine block, global data type 1001.
//!
//! Special Edition differs from Legendary in two ways that change the parse
//! rather than just the values: the string table count is a `uint32` where
//! Legendary uses a `uint16`, and a string reference inside the block is a
//! `uint32` index where Legendary uses `uint16`. ReSaver calls these `isStr32`
//! and `isID64`. Element ids are eight bytes here for the same reason.
//!
//! Layout sources: FallrimTools Papyrus.java, DefinedElement.java,
//! ScriptInstance.java, Reference.java, ArrayInfo.java and ActiveScript.java.
//! The first pass contains headers. The second pass supplies instance and
//! reference variables and array elements keyed by ID, not by header order.
//! Active stack bodies and the remaining message tables stay raw in Save.

use std::collections::{BTreeMap, BTreeSet};

use crate::cursor::{Cursor, RefId};
use crate::error::{Error, Result};

pub mod restore;
mod values;
pub use values::{Value, ValueType};

/// A reference into the block's string table.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct StringRef(pub u32);

#[derive(Clone, Debug)]
pub struct MemberDesc {
    pub name: StringRef,
    pub type_name: StringRef,
}

#[derive(Clone, Debug)]
pub struct ScriptDef {
    pub name: StringRef,
    pub parent: StringRef,
    pub members: Vec<MemberDesc>,
}

/// One live script object: which script it runs, and which form it is on.
#[derive(Clone, Debug)]
pub struct ScriptInstance {
    pub id: u64,
    pub script: StringRef,
    pub unknown_2bits: u16,
    pub unknown: u16,
    pub form: RefId,
    pub unknown_byte: u8,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub id: u64,
    pub script: StringRef,
}

#[derive(Clone, Debug)]
pub struct ArrayInfo {
    pub id: u64,
    pub element_type: ValueType,
    pub type_name: Option<StringRef>,
    pub length: u32,
}

/// Active-script IDs are 32 bits even though object and array IDs are 64 bits.
#[derive(Clone, Debug)]
pub struct ActiveScript {
    pub id: u32,
    pub kind: u8,
}

/// Shared wire layout for instance data and reference data.
#[derive(Clone, Debug)]
pub struct VariableData {
    pub id: u64,
    pub flags: u8,
    /// Current state for instances, type name for references.
    pub state_or_type: StringRef,
    pub unknown: u32,
    /// Present when flag bit 2 is set.
    pub extra: Option<u32>,
    pub variables: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct ArrayData {
    pub id: u64,
    pub variables: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct Papyrus {
    /// The VM version, or -1 in a save the game itself considers broken.
    pub vm_version: u16,
    pub strings: Vec<String>,
    pub scripts: Vec<ScriptDef>,
    pub instances: Vec<ScriptInstance>,
    pub references: Vec<Reference>,
    pub arrays: Vec<ArrayInfo>,
    pub runtime_id: u32,
    pub active_scripts: Vec<ActiveScript>,
    /// Data vectors retain second-pass file order, which can differ from headers.
    pub instance_data: Vec<VariableData>,
    pub reference_data: Vec<VariableData>,
    pub array_data: Vec<ArrayData>,
    /// First byte of the instance-variable data, after all first-pass headers.
    pub data_start: usize,
    /// Byte length of the whole block, decoded part included.
    pub block_len: usize,
    /// Byte offset where decoding stopped, so the undecoded tail is
    /// `block_len - decoded_len`.
    pub decoded_len: usize,
}

impl Papyrus {
    pub fn read(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data, "papyrus");
        let vm_version = cursor.u16()?;
        let count = read_count(&mut cursor, 2, "strings")?;
        let mut strings = Vec::with_capacity(count);
        for _ in 0..count {
            strings.push(cursor.wstring()?.to_text());
        }
        let string_count = strings.len();
        let count = read_count(&mut cursor, 12, "scripts")?;
        let mut scripts = Vec::with_capacity(count);
        for _ in 0..count {
            let name = read_string(&mut cursor, string_count)?;
            let parent = read_string(&mut cursor, string_count)?;
            let count = read_count(&mut cursor, 8, "members")?;
            let mut members = Vec::with_capacity(count);
            for _ in 0..count {
                members.push(MemberDesc {
                    name: read_string(&mut cursor, string_count)?,
                    type_name: read_string(&mut cursor, string_count)?,
                });
            }
            scripts.push(ScriptDef {
                name,
                parent,
                members,
            });
        }

        let count = read_count(&mut cursor, 20, "instances")?;
        let mut instances = Vec::with_capacity(count);
        let mut instance_ids = BTreeSet::new();
        for _ in 0..count {
            let id = cursor.u64()?;
            unique(&mut instance_ids, id, "instance")?;
            instances.push(ScriptInstance {
                id,
                script: read_string(&mut cursor, string_count)?,
                unknown_2bits: cursor.u16()?,
                unknown: cursor.u16()?,
                form: cursor.ref_id()?,
                unknown_byte: cursor.u8()?,
            });
        }

        let count = read_count(&mut cursor, 12, "references")?;
        let mut references = Vec::with_capacity(count);
        let mut reference_ids = BTreeSet::new();
        for _ in 0..count {
            let id = cursor.u64()?;
            unique(&mut reference_ids, id, "reference")?;
            references.push(Reference {
                id,
                script: read_string(&mut cursor, string_count)?,
            });
        }
        let count = read_count(&mut cursor, 13, "arrays")?;
        let mut arrays = Vec::with_capacity(count);
        let mut array_lengths = BTreeMap::new();
        for _ in 0..count {
            let id = cursor.u64()?;
            let element_type = ValueType::read(&mut cursor)?;
            let type_name = if element_type.has_type_name() {
                Some(read_string(&mut cursor, string_count)?)
            } else {
                None
            };
            let length = cursor.u32()?;
            if length as usize > cursor.remaining() / 5 {
                return Err(bad(format!("array {id:#x} length exceeds payload")));
            }
            if array_lengths.insert(id, length).is_some() {
                return Err(bad(format!("duplicate array ID {id:#x}")));
            }
            arrays.push(ArrayInfo {
                id,
                element_type,
                type_name,
                length,
            });
        }
        let runtime_id = cursor.u32()?;
        let count = read_count(&mut cursor, 5, "active scripts")?;
        let mut active_scripts = Vec::with_capacity(count);
        let mut active_ids = BTreeSet::new();
        for _ in 0..count {
            let id = cursor.u32()?;
            unique(&mut active_ids, u64::from(id), "active script")?;
            active_scripts.push(ActiveScript {
                id,
                kind: cursor.u8()?,
            });
        }

        let data_start = cursor.pos();
        let instance_data = read_variable_data(&mut cursor, string_count, instance_ids)?;
        let reference_data = read_variable_data(&mut cursor, string_count, reference_ids)?;
        let mut array_data = Vec::with_capacity(arrays.len());
        for _ in 0..arrays.len() {
            let id = cursor.u64()?;
            let count = array_lengths
                .remove(&id)
                .ok_or_else(|| bad(format!("unknown or repeated array data ID {id:#x}")))?;
            let variables = values::read_values(&mut cursor, string_count, count as usize)?;
            array_data.push(ArrayData { id, variables });
        }

        Ok(Self {
            vm_version,
            strings,
            scripts,
            instances,
            references,
            arrays,
            runtime_id,
            active_scripts,
            instance_data,
            reference_data,
            array_data,
            data_start,
            block_len: data.len(),
            decoded_len: cursor.pos(),
        })
    }

    /// The text a string reference names, when it is in range.
    pub fn string(&self, reference: StringRef) -> Option<&str> {
        self.strings.get(reference.0 as usize).map(String::as_str)
    }

    /// Re-encode the typed second pass to compare against
    /// `data[data_start..decoded_len]`. This does not rewrite the raw tail.
    pub fn value_data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for data in self.instance_data.iter().chain(&self.reference_data) {
            if (data.flags & 4 != 0) != data.extra.is_some() {
                return Err(bad("variable data flag disagrees with optional field"));
            }
            out.extend_from_slice(&data.id.to_le_bytes());
            out.push(data.flags);
            out.extend_from_slice(&data.state_or_type.0.to_le_bytes());
            out.extend_from_slice(&data.unknown.to_le_bytes());
            if let Some(extra) = data.extra {
                out.extend_from_slice(&extra.to_le_bytes());
            }
            let count =
                u32::try_from(data.variables.len()).map_err(|_| bad("too many variables"))?;
            out.extend_from_slice(&count.to_le_bytes());
            for value in &data.variables {
                value.write(&mut out)?;
            }
        }
        for data in &self.array_data {
            out.extend_from_slice(&data.id.to_le_bytes());
            for value in &data.variables {
                value.write(&mut out)?;
            }
        }
        Ok(out)
    }
}

fn bad(reason: impl Into<String>) -> Error {
    Error::Papyrus {
        reason: reason.into(),
    }
}

fn read_count(cursor: &mut Cursor<'_>, minimum: usize, name: &str) -> Result<usize> {
    let count = cursor.u32()? as usize;
    if count > cursor.remaining() / minimum {
        return Err(bad(format!("{count} {name} exceed remaining payload")));
    }
    Ok(count)
}

fn read_string(cursor: &mut Cursor<'_>, strings: usize) -> Result<StringRef> {
    let index = cursor.u32()?;
    if index as usize >= strings {
        return Err(bad(format!(
            "string index {index} is outside {strings} strings"
        )));
    }
    Ok(StringRef(index))
}

fn unique(ids: &mut BTreeSet<u64>, id: u64, kind: &str) -> Result<()> {
    if !ids.insert(id) {
        return Err(bad(format!("duplicate {kind} ID {id:#x}")));
    }
    Ok(())
}

fn read_variable_data(
    cursor: &mut Cursor<'_>,
    strings: usize,
    mut ids: BTreeSet<u64>,
) -> Result<Vec<VariableData>> {
    let count = ids.len();
    let mut data = Vec::with_capacity(count);
    for _ in 0..count {
        let id = cursor.u64()?;
        if !ids.remove(&id) {
            return Err(bad(format!("unknown or repeated variable data ID {id:#x}")));
        }
        let flags = cursor.u8()?;
        let state_or_type = read_string(cursor, strings)?;
        let unknown = cursor.u32()?;
        let extra = if flags & 4 != 0 {
            Some(cursor.u32()?)
        } else {
            None
        };
        let count = read_count(cursor, 5, "variables")?;
        let variables = values::read_values(cursor, strings, count)?;
        data.push(VariableData {
            id,
            flags,
            state_or_type,
            unknown,
            extra,
            variables,
        });
    }
    Ok(data)
}

#[cfg(test)]
mod tests;
