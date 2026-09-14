// SPDX-License-Identifier: GPL-3.0-or-later

//! Collision for the Skyrim engine: the Havok shapes Bethesda's meshes
//! carry, converted to plain geometry in game units, and a world that
//! keeps statics and moves a character capsule through them.
//!
//! Three layers:
//!
//! - `bhk` decodes the `bhk*` blocks `soe_nif` keeps as bytes.
//! - `shape` walks a parsed NIF, resolves every collision object to a
//!   [`Collider`] with a world transform and a [`Shape`], in game units.
//! - `world` is the [`PhysicsWorld`] trait and [`SimpleWorld`], a static
//!   collision world with a capsule character controller written in plain
//!   Rust. Jolt was the plan, but its Rust binding (`joltc-sys`) builds the
//!   C++ library through CMake, which this machine does not have, so the
//!   world sits behind a trait and Jolt replaces `SimpleWorld` without
//!   touching the two layers above.
//!
//! Units: the file's Havok metres become game units at the `shape` layer,
//! using the same 69.99 factor Bethesda's exporter and OpenMW use. Axes are
//! the file's, right-handed with Z up.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod bhk;
pub mod hull;
pub mod math;
pub mod shape;
pub mod world;

pub use bhk::{Layer, MotionType, UNITS_PER_METER};
pub use math::{Aabb, Iso, Mat3, Quat, Vec3};
pub use shape::{collect, collect_blocks, Collected, Collider, Shape};
pub use world::{BodyId, Character, CharacterSettings, PhysicsWorld, SimpleWorld};

#[derive(Debug)]
pub enum Error {
    Nif(soe_nif::Error),
    Unsupported(String),
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Nif(e) => write!(f, "nif: {e}"),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
            Error::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<soe_nif::Error> for Error {
    fn from(e: soe_nif::Error) -> Error {
        Error::Nif(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
