// SPDX-License-Identifier: GPL-3.0-or-later

//! Every failure carries the byte offset it was found at, in the buffer it was
//! found in. A save is two buffers, the header block read straight from the
//! file and the body which is usually compressed, so [`Error::Truncated`]
//! names which one through `region`.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("{region} truncated at byte {at}: needed {need} more bytes, {have} left")]
    Truncated {
        region: &'static str,
        at: usize,
        need: usize,
        have: usize,
    },

    #[error("invalid variable-count width tag at byte {at}")]
    VariableCount { at: usize },

    #[error("not a Skyrim save: expected the magic TESV_SAVEGAME, found {found:?}")]
    NotASave { found: String },

    #[error(
        "save format version {version} is not Special Edition, which is 12 or later (9 and below is Legendary Edition)"
    )]
    NotSpecialEdition { version: u32 },

    #[error("header declares {declared} bytes but its fields read {actual}")]
    HeaderSize { declared: u32, actual: usize },

    #[error("unknown compression type {found}, expected 0 (none), 1 (zlib) or 2 (lz4)")]
    UnknownCompression { found: u16 },

    #[error("{scheme} decompression failed: {reason}")]
    Decompress {
        scheme: &'static str,
        reason: String,
    },

    #[error("{scheme} decompressed to {got} bytes, the header said {expected}")]
    DecompressSize {
        scheme: &'static str,
        expected: u32,
        got: usize,
    },

    #[error("form version {version} is below 77, the lowest Special Edition writes")]
    FormVersion { version: u8 },

    #[error("the plugin section declares {declared} bytes but its lists read {actual}")]
    PluginInfoSize { declared: u32, actual: usize },

    #[error("change form at byte {at} has type index {index}, which names no record type")]
    ChangeFormType { at: usize, index: u8 },

    #[error(
        "change form {form:#010x} data is compressed to {got} bytes, the header said {expected}"
    )]
    ChangeFormSize {
        form: u32,
        expected: u32,
        got: usize,
    },

    #[error("file location table points {name} at byte {offset}, past the {len} byte body")]
    TableOffset {
        name: &'static str,
        offset: usize,
        len: usize,
    },

    #[error("global data block of type {kind} declares {size} bytes but only {available} remain")]
    GlobalDataOverflow {
        kind: u32,
        size: u32,
        available: usize,
    },

    #[error("global-variable table declares {count} entries but has {bytes} payload bytes")]
    GlobalVariableSize { count: u32, bytes: usize },

    #[error("the Papyrus section is {reason}")]
    Papyrus { reason: String },

    #[error("invalid reference change data: {reason}")]
    Reference { reason: String },

    #[error("invalid NPC change data: {reason}")]
    Npc { reason: String },

    #[error("invalid list change data: {reason}")]
    List { reason: String },

    #[error("invalid quest change data: {reason}")]
    Quest { reason: String },

    #[error("could not read the save file: {0}")]
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
