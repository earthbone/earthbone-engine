// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;

/// Errors from NIF and FaceGen TRI readers. Offsets are byte positions in the
/// input so malformed data can be inspected in a hex viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input ended before a field could be read.
    Truncated { offset: usize, needed: usize },
    /// The first line is not a NetImmerse or Gamebryo header string.
    BadMagic,
    /// The file is a NIF, but not a layout this crate reads.
    UnsupportedVersion {
        version: u32,
        user_version: u32,
        bs_version: u32,
    },
    /// A header field is inconsistent with the rest of the header.
    BadHeader(String),
    /// A count in the file is larger than the remaining bytes could hold.
    CountTooLarge {
        offset: usize,
        count: u64,
        element_size: usize,
    },
    /// A block ended somewhere other than where the header said it would.
    BlockSize {
        index: usize,
        type_name: String,
        expected_end: usize,
        actual_end: usize,
    },
    /// A block reference points outside the block table.
    BadRef { block: usize, reference: i32 },
    /// A string index points outside the header string table.
    BadStringIndex { offset: usize, index: u32 },
    /// A block has a type this crate parses, but the data inside it does not
    /// add up (for example a vertex description whose size does not match
    /// the attributes it declares).
    InvalidData { block: usize, reason: String },
    /// The scene graph refers to itself.
    Cycle { block: usize },
    /// A FaceGen head TRI file is not supported by this reader.
    InvalidHeadTri(String),
    /// A selected head morph does not exist in a FaceGen TRI file.
    HeadMorphNotFound(String),
    /// The supplied head mesh does not have the TRI file's vertex count.
    HeadMorphVertexCount { expected: usize, actual: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed } => {
                write!(f, "input truncated at byte {offset}, needed {needed} more")
            }
            Error::BadMagic => write!(f, "not a NIF file (no Gamebryo or NetImmerse header)"),
            Error::UnsupportedVersion {
                version,
                user_version,
                bs_version,
            } => write!(
                f,
                "unsupported NIF layout: version {version:#x}, user version {user_version}, BS version {bs_version}"
            ),
            Error::BadHeader(why) => write!(f, "bad header: {why}"),
            Error::CountTooLarge {
                offset,
                count,
                element_size,
            } => write!(
                f,
                "count {count} of {element_size}-byte elements at byte {offset} exceeds the input"
            ),
            Error::BlockSize {
                index,
                type_name,
                expected_end,
                actual_end,
            } => write!(
                f,
                "block {index} ({type_name}) should end at byte {expected_end} but parsing ended at {actual_end}"
            ),
            Error::BadRef { block, reference } => {
                write!(f, "block {block} references block {reference}, which does not exist")
            }
            Error::BadStringIndex { offset, index } => {
                write!(f, "string index {index} at byte {offset} is outside the string table")
            }
            Error::InvalidData { block, reason } => write!(f, "block {block}: {reason}"),
            Error::Cycle { block } => write!(f, "block {block} is its own ancestor"),
            Error::InvalidHeadTri(reason) => write!(f, "invalid head TRI: {reason}"),
            Error::HeadMorphNotFound(name) => write!(f, "head TRI has no morph named {name:?}"),
            Error::HeadMorphVertexCount { expected, actual } => write!(
                f,
                "head TRI has {expected} vertices but the mesh has {actual}"
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
