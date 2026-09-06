// SPDX-License-Identifier: GPL-3.0-or-later

//! The NIF header for Gamebryo 20.2.0.7 as Bethesda writes it: version
//! numbers, the Bethesda export block, the block type table, per-block
//! sizes, and the string table that every `string` field indexes.

use crate::error::{Error, Result};
use crate::stream::Stream;

/// Gamebryo 20.2.0.7, the only file version Skyrim, Fallout 3, New Vegas,
/// Fallout 4 and Fallout 76 use.
pub const VERSION_20_2_0_7: u32 = 0x1402_0007;

/// Bethesda stream versions this crate knows the block layouts for.
pub const BS_SKYRIM_LE: u32 = 83;
pub const BS_SKYRIM_SE: u32 = 100;
pub const BS_FALLOUT_4: u32 = 130;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub user_version: u32,
    /// The Bethesda stream version: 83 for Skyrim LE, 100 for SE, 130 for
    /// Fallout 4. It decides most of the layout differences.
    pub bs_version: u32,
    pub author: String,
    pub process_script: String,
    pub export_script: String,
    pub max_filepath: String,
    pub block_types: Vec<String>,
    /// Index into `block_types` for each block, in file order.
    pub block_type_index: Vec<u16>,
    pub block_sizes: Vec<u32>,
    pub strings: Vec<String>,
    pub groups: Vec<u32>,
}

impl Header {
    pub fn block_count(&self) -> usize {
        self.block_type_index.len()
    }

    /// The type name of block `i`, or an empty string if the index is bad.
    pub fn block_type(&self, i: usize) -> &str {
        self.block_type_index
            .get(i)
            .and_then(|&t| self.block_types.get((t & 0x7FFF) as usize))
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn is_sse(&self) -> bool {
        self.bs_version == BS_SKYRIM_SE
    }

    pub fn is_fo4_or_later(&self) -> bool {
        self.bs_version >= BS_FALLOUT_4
    }
}

pub fn read_header(s: &mut Stream<'_>) -> Result<Header> {
    let magic = s.line(64)?;
    let (prefix, version_text) =
        if let Some(rest) = magic.strip_prefix("Gamebryo File Format, Version ") {
            ("gamebryo", rest)
        } else if let Some(rest) = magic.strip_prefix("NetImmerse File Format, Version ") {
            ("netimmerse", rest)
        } else {
            return Err(Error::BadMagic);
        };
    let version = s.u32()?;
    if version != VERSION_20_2_0_7 || prefix != "gamebryo" {
        return Err(Error::UnsupportedVersion {
            version,
            user_version: 0,
            bs_version: 0,
        });
    }
    if version_text.trim() != "20.2.0.7" {
        return Err(Error::BadHeader(format!(
            "header line says version {} but the binary version is 20.2.0.7",
            version_text.trim()
        )));
    }
    let endian = s.u8()?;
    if endian != 1 {
        return Err(Error::BadHeader(
            "big-endian NIF files are not supported".into(),
        ));
    }
    let user_version = s.u32()?;
    let block_count = s.u32()?;

    // The Bethesda export block is present for user version 3 and above on
    // this file version, which is every Bethesda game.
    if user_version < 3 {
        return Err(Error::UnsupportedVersion {
            version,
            user_version,
            bs_version: 0,
        });
    }
    let bs_version = s.u32()?;
    let author = s.export_string()?;
    let mut process_script = String::new();
    if bs_version > 130 {
        s.u32()?;
    } else {
        process_script = s.export_string()?;
    }
    let export_script = s.export_string()?;
    let max_filepath = if bs_version >= 103 {
        s.export_string()?
    } else {
        String::new()
    };
    if !matches!(bs_version, BS_SKYRIM_LE | BS_SKYRIM_SE | BS_FALLOUT_4) {
        return Err(Error::UnsupportedVersion {
            version,
            user_version,
            bs_version,
        });
    }

    let type_count = s.u16()? as u32;
    let type_count = s.count(type_count, 4)?;
    let mut block_types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        block_types.push(s.sized_string()?);
    }
    let block_count = s.count(block_count, 2)?;
    let mut block_type_index = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let idx = s.u16()?;
        if (idx & 0x7FFF) as usize >= block_types.len() {
            return Err(Error::BadHeader(format!(
                "block type index {} is outside the {} block types",
                idx & 0x7FFF,
                block_types.len()
            )));
        }
        block_type_index.push(idx);
    }
    let mut block_sizes = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        block_sizes.push(s.u32()?);
    }

    let string_count = s.counted(4)?;
    let _max_string_length = s.u32()?;
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        strings.push(s.sized_string()?);
    }
    let group_count = s.counted(4)?;
    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        groups.push(s.u32()?);
    }

    Ok(Header {
        version,
        user_version,
        bs_version,
        author,
        process_script,
        export_script,
        max_filepath,
        block_types,
        block_type_index,
        block_sizes,
        strings,
        groups,
    })
}
