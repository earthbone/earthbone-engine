// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! A reader for Skyrim Special Edition save games.
//!
//! A save is an uncompressed header block followed by a body that is normally
//! LZ4 compressed. The body holds a location table, three tables of typed
//! global data blocks, and the change forms, which are the per object record
//! of what play has altered. Two of the global data blocks matter most here:
//! type 3 is the global variable table and type 1001 is the Papyrus virtual
//! machine.
//!
//! The format is documented on the UESP wiki and implemented in ReSaver, part
//! of Fallrim Tools. Neither was reverse engineered here, see `docs/save.md`
//! for what each contributed and where they disagree.
//!
//! The reader keeps raw bytes for everything it does not decode, so
//! [`Save::write`] reproduces a file it read byte for byte. That is the
//! strongest check available without a game to load the result, and it is what
//! the round trip test asserts.

pub mod atomic;
pub mod base_object;
pub mod change;
mod compress;
pub mod created_objects;
mod cursor;
pub mod dialogue_scene;
pub mod error;
pub mod expect;
pub mod globals;
pub mod header;
pub mod list;
pub mod npc;
pub mod papyrus;
pub mod quest;
pub mod reference;
pub mod table;

pub use base_object::{BaseObjectFields, ItemChange};
pub use change::book_note_key_flags;
pub use change::{
    ActivatorChange, BaseObjectChange, BookChange, ClassChange, FactionChange, FactionReaction,
    FactionReactions, IngredientChange, KeyChange, NoteChange, RelationshipChange,
};
pub use change::{ChangeForm, QuestStage};
pub use created_objects::{CreatedEffect, CreatedObject, CreatedObjects};
pub use cursor::{RefId, RefKind, WString};
pub use dialogue_scene::{InfoChange, SceneChange, StoryManagerQuestNodeChange};
pub use error::{Error, Result};
pub use globals::{GlobalVariable, GlobalVariables};
pub use header::{Compression, Header};
pub use papyrus::Papyrus;
pub use table::{FileLocationTable, GlobalData};

/// The lowest form version Special Edition writes.
const SE_MIN_FORM_VERSION: u8 = 77;

/// The form version at which the light plugin (ESL) list appears.
const ESL_FORM_VERSION: u8 = 78;

#[derive(Clone, Debug)]
pub struct Save {
    pub header: Header,
    pub form_version: u8,
    pub plugins: Vec<WString>,
    pub light_plugins: Vec<WString>,
    pub location_table: FileLocationTable,
    pub table_1: Vec<GlobalData>,
    pub table_2: Vec<GlobalData>,
    pub table_3: Vec<GlobalData>,
    pub change_forms: Vec<ChangeForm>,
    pub form_id_array: Vec<u32>,
    pub visited_worldspaces: Vec<u32>,
    /// Whatever follows the visited worldspace array, kept so a write is exact.
    pub unknown_3: Vec<u8>,
    /// Length of the uncompressed header block, which every location table
    /// offset is measured from.
    header_len: usize,
}

impl Save {
    /// Length of the uncompressed block at the front of the file, which is the
    /// magic, the header fields and the screenshot. Every location table
    /// offset is measured from the start of the file, so this is what turns
    /// one into an index into the body.
    pub fn header_len(&self) -> usize {
        self.header_len
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let _span = soe_trace::Span::new("save.parse", "save");
        let (header, header_len, body) = Self::split(bytes)?;
        Self::parse_body(header, header_len, &body)
    }

    /// Splits a file into its header, the header block's length, and the
    /// decompressed body.
    ///
    /// Exposed because the body is the part worth comparing byte for byte. The
    /// file itself cannot be, since rebuilding it re-runs the compressor and
    /// nothing promises an encoder reproduces the game's exact output.
    pub fn split(bytes: &[u8]) -> Result<(Header, usize, Vec<u8>)> {
        let mut file = cursor::Cursor::new(bytes, "header");
        let header = Header::read(&mut file)?;
        let header_len = file.pos();

        let body = match header.compression {
            Compression::None => file.rest().to_vec(),
            scheme => {
                let uncompressed_len = file.u32()?;
                let compressed_len = file.u32()?;
                let compressed = file.take(compressed_len as usize)?;
                let (name, inflated) = match scheme {
                    Compression::Zlib => (
                        "zlib",
                        compress::inflate_zlib(compressed, uncompressed_len)?,
                    ),
                    _ => ("lz4", compress::inflate_lz4(compressed, uncompressed_len)?),
                };
                if inflated.len() != uncompressed_len as usize {
                    return Err(Error::DecompressSize {
                        scheme: name,
                        expected: uncompressed_len,
                        got: inflated.len(),
                    });
                }
                inflated
            }
        };

        Ok((header, header_len, body))
    }

    /// The body as this reader would write it. Compare against the body from
    /// [`Save::split`] to prove nothing was lost in the parse.
    pub fn body_bytes(&self) -> Vec<u8> {
        self.write_body()
    }

    fn parse_body(header: Header, header_len: usize, body: &[u8]) -> Result<Self> {
        let mut cursor = cursor::Cursor::new(body, "body");

        let form_version = cursor.u8()?;
        if form_version < SE_MIN_FORM_VERSION {
            return Err(Error::FormVersion {
                version: form_version,
            });
        }

        // The plugin lists are wrapped in a byte length. Both lists sit inside
        // it, so the count is a check on the whole section rather than on the
        // full list alone.
        let plugin_info_size = cursor.u32()?;
        let plugin_info_start = cursor.pos();

        let plugin_count = cursor.u8()?;
        let mut plugins = Vec::with_capacity(plugin_count as usize);
        for _ in 0..plugin_count {
            plugins.push(cursor.wstring()?);
        }

        let mut light_plugins = Vec::new();
        if form_version >= ESL_FORM_VERSION {
            let light_count = cursor.u16()?;
            light_plugins.reserve(light_count as usize);
            for _ in 0..light_count {
                light_plugins.push(cursor.wstring()?);
            }
        }

        let plugin_info_read = cursor.pos() - plugin_info_start;
        if plugin_info_read != plugin_info_size as usize {
            return Err(Error::PluginInfoSize {
                declared: plugin_info_size,
                actual: plugin_info_read,
            });
        }

        let location_table = FileLocationTable::read(&mut cursor)?;

        // The form id array sits past the change forms, but a reference in any
        // earlier table needs it to resolve, so read it first and come back.
        let form_id_array_at = offset_into_body(
            location_table.form_id_array_count_offset,
            header_len,
            "form id array",
        )?;
        cursor.seek(form_id_array_at, "form id array")?;
        let form_id_count = cursor.u32()?;
        if form_id_count as usize * 4 > cursor.remaining() {
            return Err(Error::TableOffset {
                name: "form id array",
                offset: form_id_count as usize * 4,
                len: cursor.remaining(),
            });
        }
        let mut form_id_array = Vec::with_capacity(form_id_count as usize);
        for _ in 0..form_id_count {
            form_id_array.push(cursor.u32()?);
        }

        let worldspace_count = cursor.u32()?;
        let mut visited_worldspaces = Vec::with_capacity(worldspace_count as usize);
        for _ in 0..worldspace_count {
            visited_worldspaces.push(cursor.u32()?);
        }
        let unknown_3 = cursor.rest().to_vec();

        // Back to the tables, which run in one stretch from table one through
        // the change forms to table three.
        let table_1_at = offset_into_body(location_table.table_1_offset, header_len, "table 1")?;
        cursor.seek(table_1_at, "table 1")?;

        let mut table_1 = Vec::with_capacity(location_table.table_1_count as usize);
        for _ in 0..location_table.table_1_count {
            table_1.push(GlobalData::read(&mut cursor)?);
        }
        let mut table_2 = Vec::with_capacity(location_table.table_2_count as usize);
        for _ in 0..location_table.table_2_count {
            table_2.push(GlobalData::read(&mut cursor)?);
        }
        let mut change_forms = Vec::with_capacity(location_table.change_form_count as usize);
        for _ in 0..location_table.change_form_count {
            change_forms.push(ChangeForm::read(&mut cursor)?);
        }
        let mut table_3 = Vec::with_capacity(location_table.table_3_read_count() as usize);
        for _ in 0..location_table.table_3_read_count() {
            table_3.push(GlobalData::read(&mut cursor)?);
        }

        Ok(Save {
            header,
            form_version,
            plugins,
            light_plugins,
            location_table,
            table_1,
            table_2,
            table_3,
            change_forms,
            form_id_array,
            visited_worldspaces,
            unknown_3,
            header_len,
        })
    }

    /// Rebuilds the file. The body is re-compressed with the scheme the header
    /// names, so the bytes match only when that scheme is deterministic. LZ4
    /// block is, which is what Special Edition uses.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.header.write(&mut out);

        let body = self.write_body();
        match self.header.compression {
            Compression::None => out.extend_from_slice(&body),
            scheme => {
                let compressed = match scheme {
                    Compression::Zlib => compress::deflate_zlib(&body)?,
                    _ => compress::deflate_lz4(&body),
                };
                out.extend_from_slice(&(body.len() as u32).to_le_bytes());
                out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
                out.extend_from_slice(&compressed);
            }
        }
        Ok(out)
    }

    /// Write an ESS through validated atomic replacement.
    ///
    /// Validation reparses the file, checks every compressed change payload,
    /// and requires exact re-encoding. Opaque change data remains opaque, as
    /// in [`Save::parse`]. This does not claim complete runtime restoration.
    ///
    /// # Errors
    /// Serialization, read-back validation or a persistence operation fails.
    pub fn write_file(
        &self,
        path: &std::path::Path,
    ) -> std::result::Result<(), atomic::WriteError> {
        let bytes = self
            .write()
            .map_err(|error| atomic::WriteError::Serialize(error.to_string()))?;
        atomic::write_validated(path, &bytes, |written| {
            let decoded = Self::parse(written).map_err(|error| error.to_string())?;
            for change in &decoded.change_forms {
                change.payload().map_err(|error| error.to_string())?;
            }
            if decoded.write().map_err(|error| error.to_string())? != written {
                return Err("ESS read-back does not reproduce the complete file".into());
            }
            Ok(())
        })
    }

    fn write_body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(self.form_version);

        let mut plugin_info = Vec::new();
        plugin_info.push(self.plugins.len() as u8);
        for plugin in &self.plugins {
            plugin.write(&mut plugin_info);
        }
        if self.form_version >= ESL_FORM_VERSION {
            plugin_info.extend_from_slice(&(self.light_plugins.len() as u16).to_le_bytes());
            for plugin in &self.light_plugins {
                plugin.write(&mut plugin_info);
            }
        }
        out.extend_from_slice(&(plugin_info.len() as u32).to_le_bytes());
        out.extend_from_slice(&plugin_info);
        self.location_table.write(&mut out);
        for block in &self.table_1 {
            block.write(&mut out);
        }
        for block in &self.table_2 {
            block.write(&mut out);
        }
        for form in &self.change_forms {
            form.write(&mut out);
        }
        for block in &self.table_3 {
            block.write(&mut out);
        }
        out.extend_from_slice(&(self.form_id_array.len() as u32).to_le_bytes());
        for form in &self.form_id_array {
            out.extend_from_slice(&form.to_le_bytes());
        }
        out.extend_from_slice(&(self.visited_worldspaces.len() as u32).to_le_bytes());
        for world in &self.visited_worldspaces {
            out.extend_from_slice(&world.to_le_bytes());
        }
        out.extend_from_slice(&self.unknown_3);
        out
    }

    /// Turns a save reference into the FormID the running game would use.
    pub fn resolve(&self, reference: RefId) -> Option<u32> {
        if reference.is_zero() {
            return Some(0);
        }
        match reference.kind() {
            RefKind::Index => {
                let index = reference.value();
                if index == 0 {
                    Some(0)
                } else {
                    self.form_id_array.get(index as usize - 1).copied()
                }
            }
            // Plugin index 0, which is Skyrim.esm in every save.
            RefKind::Default => Some(reference.value()),
            RefKind::Created => Some(0xFF00_0000 | reference.value()),
            RefKind::Invalid => None,
        }
    }

    /// The global variable table, empty when the save has none.
    pub fn globals(&self) -> Result<GlobalVariables> {
        match self
            .table_1
            .iter()
            .find(|block| block.kind == GlobalData::GLOBAL_VARIABLES)
        {
            Some(block) => GlobalVariables::read(&block.data),
            None => Ok(GlobalVariables::default()),
        }
    }

    /// The Papyrus block, absent when the save carries none.
    pub fn papyrus(&self) -> Option<Result<Papyrus>> {
        self.table_3
            .iter()
            .find(|block| block.kind == GlobalData::PAPYRUS)
            .map(|block| Papyrus::read(&block.data))
    }

    /// Every quest whose change form records stage state, with its stages.
    pub fn quest_stages(&self) -> Result<Vec<(u32, Vec<QuestStage>)>> {
        let mut out = Vec::new();
        for form in &self.change_forms {
            let Some(stages) = form.quest_stages()? else {
                continue;
            };
            let Some(id) = self.resolve(form.form) else {
                continue;
            };
            out.push((id, stages));
        }
        Ok(out)
    }

    /// Count of change forms by record type, highest first.
    pub fn change_form_counts(&self) -> Vec<(&'static str, usize)> {
        let mut counts: std::collections::BTreeMap<&'static str, usize> =
            std::collections::BTreeMap::new();
        for form in &self.change_forms {
            *counts.entry(form.record_type()).or_default() += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        counts
    }
}

/// A location table offset is an absolute position in the file as it would
/// look uncompressed. The body buffer starts after the header block, so the
/// index into it is the offset minus the header length.
fn offset_into_body(offset: u32, header_len: usize, name: &'static str) -> Result<usize> {
    (offset as usize)
        .checked_sub(header_len)
        .ok_or(Error::TableOffset {
            name,
            offset: offset as usize,
            len: header_len,
        })
}
