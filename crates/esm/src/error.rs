// SPDX-License-Identifier: GPL-3.0-or-later

//! Every failure carries the absolute byte offset it was found at, so a
//! malformed plugin can be opened in a hex editor at the reported position.

use crate::tag::Tag;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("truncated at byte {at}: needed {need} more bytes, {have} left")]
    Truncated { at: usize, need: usize, have: usize },

    #[error("expected a TES4 header record at the start of the file, found {found} at byte {at}")]
    NoHeader { at: usize, found: Tag },

    #[error("group at byte {at} declares {size} bytes but only {available} remain")]
    GroupOverflow {
        at: usize,
        size: u32,
        available: usize,
    },

    #[error("group at byte {at} declares {size} bytes, smaller than its own header")]
    GroupTooSmall { at: usize, size: u32 },

    #[error("unknown group type {kind} at byte {at}")]
    UnknownGroupType { at: usize, kind: i32 },

    #[error("record {tag} at byte {at} declares {size} bytes but only {available} remain")]
    RecordOverflow {
        at: usize,
        tag: Tag,
        size: u32,
        available: usize,
    },

    #[error("compressed record {tag} at byte {at} could not be inflated: {reason}")]
    Inflate { at: usize, tag: Tag, reason: String },

    #[error(
        "compressed record {tag} at byte {at} inflated to {got} bytes, header said {expected}"
    )]
    InflateSize {
        at: usize,
        tag: Tag,
        expected: u32,
        got: usize,
    },

    #[error("subrecord {tag} at byte {at} (inside record {record}) runs {overshoot} bytes past its record")]
    SubrecordOverflow {
        at: usize,
        tag: Tag,
        record: Tag,
        overshoot: usize,
    },

    #[error("XXXX size marker at byte {at} is not followed by a subrecord")]
    DanglingXxxx { at: usize },

    #[error("subrecord {tag} in record {record} is {len} bytes, {expected} expected")]
    SubrecordSize {
        record: Tag,
        tag: Tag,
        len: usize,
        expected: String,
    },

    #[error("record {record} has no {tag} subrecord, which is required")]
    MissingSubrecord { record: Tag, tag: Tag },

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
