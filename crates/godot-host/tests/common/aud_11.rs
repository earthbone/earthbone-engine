// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Installed host checks require a current release extension before running scripts.
//! Project registration follows the shared isolated Godot check helper.

#[path = "prepared_extension.rs"]
pub mod prepared_extension;

#[path = "isolated_project.rs"]
mod isolated_project;

pub fn project(scripts: &[&str]) -> tempfile::TempDir {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // CONTEXT: Debug gameplay startup can exceed the automated Godot time limit.
    let library = prepared_extension::require(&workspace);
    let mut scripts = scripts.to_vec();
    if scripts.contains(&"walkthrough.gd") {
        for dependency in [
            "record_sky.gd",
            "record_sky.gdshader",
            "record_fog.gdshader",
        ] {
            if !scripts.contains(&dependency) {
                scripts.push(dependency);
            }
        }
    }
    isolated_project::project(&scripts, Some(&library))
}
