// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot 4 host for the open Skyrim engine.
//!
//! The crate is two layers. `units`, `streaming`, `mesh`, `terrain` and
//! `terrain_lod` are plain Rust with no engine dependency and carry all the
//! logic: which cells to keep loaded, how a decoded NIF shape becomes mesh
//! arrays, how LAND heights become Terrain3D regions, which distant-terrain
//! LOD blocks replace them at range. `godot` (behind the feature of the
//! same name, on by default) is the thin layer that exposes them as a
//! `CellStreamer` node and two conversion functions. It compiles without a
//! Godot binary and is only exercised once the extension is loaded.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod actor;
pub mod animated_static;
pub mod audio;
pub mod batch;
pub mod camera;
pub mod collision;
pub mod combat;
pub mod data_source;
pub mod dialogue_menu;
pub mod door_fade;
pub mod image_space;
pub mod input;
pub mod interior_prefetch;
pub mod inventory_menu;
pub mod magic_menu;
pub mod mesh;
pub mod skinned;
pub mod streaming;
pub mod terrain;
pub mod terrain_lod;
pub mod ui;
pub mod units;

#[cfg(feature = "godot")]
pub mod godot_actor;
#[cfg(feature = "godot")]
pub mod godot_animated_static;
#[cfg(feature = "godot")]
pub mod godot_audio;
#[cfg(feature = "godot")]
pub mod godot_combat;
#[cfg(feature = "godot")]
pub mod godot_layer;
#[cfg(feature = "godot")]
pub mod godot_physics;
#[cfg(feature = "godot")]
pub mod godot_player;
#[cfg(feature = "godot")]
pub mod godot_tether;
#[cfg(feature = "godot")]
mod godot_world;

#[cfg(feature = "godot")]
mod entry {
    use godot::prelude::*;

    struct SoeHost;

    #[gdextension]
    unsafe impl ExtensionLibrary for SoeHost {}
}
