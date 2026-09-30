// SPDX-License-Identifier: MIT

//! The Rust side of a source-level CommonLibSSE-NG compatibility shim.
//!
//! SKSE plugins are recompiled against C++ headers that present the `RE::`
//! and `SKSE::` surface over this crate. The headers are generated later,
//! once the engine's runtime types are fixed. What lives here is the shape
//! those headers bind to, one module per header group in `docs/shim.md`,
//! and a [`support`] table that keeps the document and the code in step.
//!
//! This crate is MIT so plugin authors can link it. It depends on nothing
//! engine-side: the engine implements the traits, the shim only names them.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod events;
pub mod form;
pub mod headers;
pub mod ni;
pub mod refr;
pub mod skse;
pub mod support;
pub mod util;
pub mod vm;
pub mod world_types;
