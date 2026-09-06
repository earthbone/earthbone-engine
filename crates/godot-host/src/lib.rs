// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot 4 host for the open Skyrim engine.
//!
//! The crate is two layers. `units`, `streaming`, `mesh` and `terrain` are
//! plain Rust with no engine dependency and carry all the logic: which
//! cells to keep loaded, how a decoded NIF shape becomes mesh arrays, how
//! LAND heights become Terrain3D regions. `godot` (behind the feature of the
//! same name, on by default) is the thin layer that exposes them as a
//! `CellStreamer` node and two conversion functions. It compiles without a
//! Godot binary and is only exercised once the extension is loaded.

pub mod mesh;
pub mod streaming;
pub mod terrain;
pub mod units;

#[cfg(feature = "godot")]
pub mod godot_layer;

#[cfg(feature = "godot")]
mod entry {
    use godot::prelude::*;

    struct SoeHost;

    #[gdextension]
    unsafe impl ExtensionLibrary for SoeHost {}
}
