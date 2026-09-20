// SPDX-License-Identifier: GPL-3.0-or-later

//! The file location table and the global data blocks it indexes.
//!
//! The table's offsets are absolute positions in the file as it would look
//! uncompressed, so an offset into the body is the stored value minus the
//! length of the header block. The three counts are split by type range: table
//! one holds types 0 to 99, table two 100 to 999, table three 1000 and up.
//!
//! The third count is short by one in every Skyrim save. The game does not
//! count type 1001, the Papyrus block, which means it never reads the last
//! entry of that table. ReSaver adds one back when reading and this does the
//! same, so the Papyrus block is reachable.

use crate::cursor::Cursor;
use crate::error::{Error, Result};

#[derive(Clone, Copy, Debug)]
pub struct FileLocationTable {
    pub form_id_array_count_offset: u32,
    pub unknown_table_3_offset: u32,
    pub table_1_offset: u32,
    pub table_2_offset: u32,
    pub change_forms_offset: u32,
    pub table_3_offset: u32,
    pub table_1_count: u32,
    pub table_2_count: u32,
    /// As stored. [`Self::table_3_read_count`] is the number to actually read.
    pub table_3_count: u32,
    pub change_form_count: u32,
    pub unused: [u32; 15],
}

impl FileLocationTable {
    /// The stored third count plus the Papyrus block the game forgets.
    pub fn table_3_read_count(&self) -> u32 {
        self.table_3_count + 1
    }

    pub fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let form_id_array_count_offset = cursor.u32()?;
        let unknown_table_3_offset = cursor.u32()?;
        let table_1_offset = cursor.u32()?;
        let table_2_offset = cursor.u32()?;
        let change_forms_offset = cursor.u32()?;
        let table_3_offset = cursor.u32()?;
        let table_1_count = cursor.u32()?;
        let table_2_count = cursor.u32()?;
        let table_3_count = cursor.u32()?;
        let change_form_count = cursor.u32()?;
        let mut unused = [0u32; 15];
        for slot in &mut unused {
            *slot = cursor.u32()?;
        }
        Ok(FileLocationTable {
            form_id_array_count_offset,
            unknown_table_3_offset,
            table_1_offset,
            table_2_offset,
            change_forms_offset,
            table_3_offset,
            table_1_count,
            table_2_count,
            table_3_count,
            change_form_count,
            unused,
        })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        for value in [
            self.form_id_array_count_offset,
            self.unknown_table_3_offset,
            self.table_1_offset,
            self.table_2_offset,
            self.change_forms_offset,
            self.table_3_offset,
            self.table_1_count,
            self.table_2_count,
            self.table_3_count,
            self.change_form_count,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in self.unused {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

/// A typed, length-prefixed block. The payload stays raw here, the types that
/// matter are decoded by their own modules from these bytes.
#[derive(Clone, Debug)]
pub struct GlobalData {
    pub kind: u32,
    pub data: Vec<u8>,
}

impl GlobalData {
    /// Type 3 is the global variable table.
    pub const GLOBAL_VARIABLES: u32 = 3;
    /// Type 4 holds created enchantments, potions and poisons.
    pub const CREATED_OBJECTS: u32 = 4;
    /// Type 1001 is the Papyrus virtual machine state.
    pub const PAPYRUS: u32 = 1001;

    pub fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let kind = cursor.u32()?;
        let size = cursor.u32()?;
        if size as usize > cursor.remaining() {
            return Err(Error::GlobalDataOverflow {
                kind,
                size,
                available: cursor.remaining(),
            });
        }
        let data = cursor.take(size as usize)?.to_vec();
        Ok(GlobalData { kind, data })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.data);
    }
}
