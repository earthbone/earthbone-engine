// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Run the collision script against Cargo's current extension in isolation.

use std::process::{Command, Output};

#[path = "aud_11.rs"]
mod extension;

pub fn installed_collision() -> Option<Output> {
    let data = std::env::var_os("SKYRIM_DATA")?;
    let godot = std::env::var_os("SOE_GODOT_BIN")?;
    let project = extension::project(&["physics_test.gd"]);
    Some(
        Command::new(godot)
            .args(["--headless", "--path"])
            .arg(project.path())
            .args(["--script", "physics_test.gd"])
            .env("SKYRIM_DATA", data)
            .env("SOE_LOG", "warn")
            .env("XDG_DATA_HOME", project.path())
            .env("GODOT_SILENCE_ROOT_WARNING", "1")
            .output()
            .expect("Godot process starts"),
    )
}
