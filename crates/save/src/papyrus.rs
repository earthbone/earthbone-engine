// SPDX-License-Identifier: GPL-3.0-or-later

//! The Papyrus virtual machine block, global data type 1001.
//!
//! Special Edition differs from Legendary in two ways that change the parse
//! rather than just the values: the string table count is a `uint32` where
//! Legendary uses a `uint16`, and a string reference inside the block is a
//! `uint32` index where Legendary uses `uint16`. ReSaver calls these `isStr32`
//! and `isID64`. Element ids are eight bytes here for the same reason.
//!
//! What is decoded: the string table, the script definitions with their member
//! descriptors, and the script instances that bind a script to a form. That is
//! the part a conformance check wants, since it answers which script is
//! attached to which object and what its declared members are, which is the
//! same question `BindReport` answers on our side.
//!
//! What is left raw: everything from the reference table onward, which is the
//! arrays, the active scripts and the per instance variable data. The variable
//! data is the natural next step and it is reachable only by walking the
//! active script and stack structures first, since the file stores instance
//! variables in a second pass after them rather than next to their instance.

use crate::cursor::{Cursor, RefId};
use crate::error::{Error, Result};

/// A reference into the block's string table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
pub struct Papyrus {
    /// The VM version, or -1 in a save the game itself considers broken.
    pub vm_version: u16,
    pub strings: Vec<String>,
    pub scripts: Vec<ScriptDef>,
    pub instances: Vec<ScriptInstance>,
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

        let string_count = cursor.u32()?;
        // A save's string table is large but bounded. A wild count means the
        // block is not what we think it is, and allocating on it would be an
        // out of memory rather than a parse error.
        if string_count as usize > data.len() {
            return Err(Error::Papyrus {
                reason: format!(
                    "declaring {string_count} strings in a {} byte block",
                    data.len()
                ),
            });
        }
        let mut strings = Vec::with_capacity(string_count as usize);
        for _ in 0..string_count {
            strings.push(cursor.wstring()?.to_text());
        }

        let script_count = cursor.u32()?;
        if script_count as usize > data.len() {
            return Err(Error::Papyrus {
                reason: format!(
                    "declaring {script_count} scripts in a {} byte block",
                    data.len()
                ),
            });
        }
        let mut scripts = Vec::with_capacity(script_count as usize);
        for _ in 0..script_count {
            let name = StringRef(cursor.u32()?);
            let parent = StringRef(cursor.u32()?);
            let member_count = cursor.u32()?;
            if member_count as usize > data.len() {
                return Err(Error::Papyrus {
                    reason: format!("a script declaring {member_count} members"),
                });
            }
            let mut members = Vec::with_capacity(member_count as usize);
            for _ in 0..member_count {
                members.push(MemberDesc {
                    name: StringRef(cursor.u32()?),
                    type_name: StringRef(cursor.u32()?),
                });
            }
            scripts.push(ScriptDef {
                name,
                parent,
                members,
            });
        }

        let instance_count = cursor.u32()?;
        if instance_count as usize > data.len() {
            return Err(Error::Papyrus {
                reason: format!("declaring {instance_count} script instances"),
            });
        }
        let mut instances = Vec::with_capacity(instance_count as usize);
        for _ in 0..instance_count {
            let id = cursor.u64()?;
            let script = StringRef(cursor.u32()?);
            let unknown_2bits = cursor.u16()?;
            let unknown = cursor.u16()?;
            let form = cursor.ref_id()?;
            let unknown_byte = cursor.u8()?;
            instances.push(ScriptInstance {
                id,
                script,
                unknown_2bits,
                unknown,
                form,
                unknown_byte,
            });
        }

        Ok(Papyrus {
            vm_version,
            strings,
            scripts,
            instances,
            block_len: data.len(),
            decoded_len: cursor.pos(),
        })
    }

    /// The text a string reference names, when it is in range.
    pub fn string(&self, reference: StringRef) -> Option<&str> {
        self.strings.get(reference.0 as usize).map(String::as_str)
    }
}
