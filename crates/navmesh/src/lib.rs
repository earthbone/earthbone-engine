// SPDX-License-Identifier: GPL-3.0-or-later

//! Navigation meshes for the engine, in two halves.
//!
//! The vanilla half ([`vanilla`]) turns the `NAVM` records `soe-esm` already
//! parses into a walkable [`graph::NavGraph`], stitched across navmeshes
//! through their edge links, so AI packages written against the shipped
//! navmeshes have their data. The generation half ([`generate`]) builds a new
//! navmesh from a triangle [`scene::Scene`] (NIF meshes plus `LAND` heights)
//! with Recast, through the `rerecast` port, and can write the result back
//! as an `NVNM` block, which is what mods with broken or missing navmeshes
//! need and what the Creation Kit used to be the only source of.
//!
//! Every coordinate at this crate's boundary is in Skyrim's frame: right
//! handed, Z up, one unit is one game unit (a cell is 4096 of them). Recast
//! is Y up, and the conversion is private to [`generate`].

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod generate;
pub mod graph;
pub mod scene;
pub mod stitch;
pub mod vanilla;

pub use generate::{generate, generate_cell, Agent, Navmesh};
pub use graph::{NavGraph, NodeId, Path};
pub use scene::Scene;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A step of the Recast pipeline refused its input.
    #[error("navmesh generation: {0}")]
    Generate(String),
    /// The scene has nothing to rasterize.
    #[error("the scene has no triangles")]
    EmptyScene,
    /// Agent parameters that cannot produce a mesh.
    #[error("agent parameters: {0}")]
    Agent(String),
    /// A generated mesh does not fit the `NVNM` limits.
    #[error("cannot export to NVNM: {0}")]
    Export(String),
}

pub type Result<T> = std::result::Result<T, Error>;
