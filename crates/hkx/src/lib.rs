// SPDX-License-Identifier: GPL-3.0-or-later

//! Havok 2010.2 packfile reader for Skyrim Special Edition.
//!
//! Skyrim SE ships its skeletons, animations, ragdolls and behavior graphs as
//! Havok `hkx` packfiles in the `hk_2010.2.0-r1` binary layout with 64-bit
//! pointers. This crate reads that layout without any Havok SDK: the field
//! offsets come from the class descriptions serde-hkx extracted from the
//! files themselves and from the reverse-engineered runtime layouts in
//! activeragdoll's `havok_behavior.h`.
//!
//! Layers, from the bottom:
//!
//! - [`packfile`]: the container. Header, sections, class names and the three
//!   fixup tables, plus a bounds-checked view over one object in the data
//!   section. Nothing here knows what a skeleton is.
//! - [`skeleton`], [`animation`], [`spline`]: `hkaSkeleton`,
//!   `hkaAnimationBinding` and `hkaSplineCompressedAnimation`, with the spline
//!   block decoder that turns quantized control points into poses.
//! - [`ragdoll`]: rigid bodies, shapes, joint frames and limits from skeleton files.
//! - [`behavior`]: the `hkb` behavior graph as a typed data model.
//! - [`interp`]: the interpreter over that model, one instance per actor,
//!   ticked per frame into a local pose (see `docs/hkx.md`).
//! - [`character`]: `hkbCharacterData` and `hkbProjectData`, the files that
//!   tie a rig, a behavior and an animation set together.
//! - [`writer`]: a packfile builder, used by the tests to make synthetic
//!   files and usable later for patching.
//!
//! Every parser returns [`Error`] on malformed input rather than panicking,
//! and every read is bounds-checked against the data section.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod animation;
pub mod behavior;
pub mod character;
pub mod interp;
pub mod link;
pub mod packfile;
pub mod pose;
pub mod ragdoll;
pub mod skeleton;
pub mod spline;
pub mod writer;

use std::fmt;

pub use packfile::{Obj, Packfile};

/// A quaternion-scale-translation transform, Havok's `hkQsTransform`.
/// Rotation is `(x, y, z, w)`. All three fields are 16 bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QsTransform {
    pub translation: [f32; 4],
    pub rotation: [f32; 4],
    pub scale: [f32; 4],
}

impl QsTransform {
    pub const IDENTITY: QsTransform = QsTransform {
        translation: [0.0, 0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0, 0.0],
    };
}

/// Everything that can go wrong reading a packfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file does not start with the two Havok magic numbers.
    NotAPackfile,
    /// A read went past the end of the buffer or section.
    Truncated {
        what: &'static str,
        at: usize,
        need: usize,
        have: usize,
    },
    /// Only little-endian files are read. Skyrim on PC never wrote big-endian.
    BigEndian,
    /// The layouts in this crate are for 64-bit pointers. Skyrim LE files
    /// are 32-bit and need the other layout table.
    PointerSize(u8),
    /// A fixup points outside the section it names.
    BadFixup {
        kind: &'static str,
        src: u32,
        dst: u32,
    },
    /// The file's section table has no section with this tag.
    MissingSection(&'static str),
    /// A pointer resolved to an object of the wrong class.
    WrongClass {
        expected: &'static str,
        found: String,
    },
    /// A required pointer was null.
    NullPointer(&'static str),
    /// A string was not valid UTF-8 or had no terminator.
    BadString(u32),
    /// A quantization or animation type this crate does not decode yet.
    Unsupported(String),
    /// A spline block is inconsistent with its animation header.
    BadSpline(String),
    /// A ragdoll has inconsistent body, bone, or constraint references.
    BadRagdoll(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAPackfile => write!(f, "not a Havok packfile (bad magic)"),
            Error::Truncated {
                what,
                at,
                need,
                have,
            } => write!(
                f,
                "truncated {what}: need {need} bytes at {at}, have {have}"
            ),
            Error::BigEndian => write!(f, "big-endian packfiles are not supported"),
            Error::PointerSize(n) => write!(
                f,
                "{n}-byte pointers: only the 64-bit Skyrim SE layout is supported"
            ),
            Error::BadFixup { kind, src, dst } => {
                write!(
                    f,
                    "{kind} fixup at {src} points outside its section ({dst})"
                )
            }
            Error::MissingSection(tag) => write!(f, "no {tag} section"),
            Error::WrongClass { expected, found } => {
                write!(f, "expected a {expected}, found {found}")
            }
            Error::NullPointer(what) => write!(f, "{what} is null"),
            Error::BadString(at) => write!(f, "bad string at data offset {at}"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::BadSpline(what) => write!(f, "bad spline data: {what}"),
            Error::BadRagdoll(what) => write!(f, "bad ragdoll data: {what}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
