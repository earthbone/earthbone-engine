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

#[cfg(any(debug_assertions, feature = "profile-memory"))]
mod allocations;

pub mod actor;
pub mod animated_static;
pub mod animation_dispatch;
pub mod audio;
pub mod batch;
pub mod camera;
#[cfg(feature = "godot")]
mod carry;
#[cfg(feature = "godot")]
mod cell_lighting;
pub mod collision;
pub mod combat;
pub mod crafting_menu;
pub mod data_source;
pub mod detect_life;
pub mod detection;
pub mod dialogue_menu;
pub mod door_fade;
pub mod effect_shader;
pub mod facegen;
pub mod facial;
pub mod hud;
pub mod image_space;
pub mod input;
pub mod interior_prefetch;
pub mod inventory_menu;
pub mod item_card;
pub mod item_menu;
pub mod lod_rebuild;
pub mod magic_menu;
pub mod mcm_menu;
pub mod mesh;
pub mod nif_effect;
pub mod nordic_theme;
pub mod object_animation;
pub mod objective_map;
pub mod quest_journal;
pub mod quick_loot;
pub mod race_menu;
pub mod ragdoll;
pub mod save_recovery;
pub mod skinned;
pub mod streaming;
pub mod terrain;
pub mod terrain_lod;
pub mod terrain_lod_generate;
pub mod training_menu;
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
pub mod godot_forms;
#[cfg(feature = "godot")]
pub mod godot_layer;
#[cfg(feature = "godot")]
pub mod godot_logging;
#[cfg(feature = "godot")]
pub mod godot_particle;
#[cfg(feature = "godot")]
pub mod godot_physics;
#[cfg(feature = "godot")]
pub mod godot_player;
#[cfg(feature = "godot")]
mod godot_profiler;
#[cfg(feature = "godot")]
pub mod godot_ragdoll;
#[cfg(feature = "godot")]
pub mod godot_reference_editor;
#[cfg(feature = "godot")]
mod godot_sky;
#[cfg(feature = "godot")]
pub mod godot_tether;
#[cfg(feature = "godot")]
mod godot_world;

#[cfg(feature = "godot")]
mod entry {
    use godot::prelude::*;

    struct SoeHost;

    #[gdextension]
    unsafe impl ExtensionLibrary for SoeHost {
        fn on_stage_init(level: InitStage) {
            if level == InitStage::Scene {
                crate::godot_logging::install();
                crate::godot_profiler::register();
            }
        }

        fn on_stage_deinit(level: InitStage) {
            if level == InitStage::Scene {
                crate::godot_profiler::unregister();
                crate::godot_logging::flush(true);
            }
        }
    }
}

#[cfg(feature = "godot")]
mod scene_timing;
