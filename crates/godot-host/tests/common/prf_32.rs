// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Run the collision script against Cargo's current extension in isolation.

use std::path::Path;
use std::process::{Command, Output};

pub fn installed_collision() -> Option<Output> {
    let data = std::env::var_os("SKYRIM_DATA")?;
    let godot =
        std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required for the host check");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = std::env::current_exe().expect("test executable path");
    let library = executable
        .parent()
        .expect("test executable directory")
        .join("libsoe_godot_host.so");
    assert!(library.is_file(), "missing extension {}", library.display());
    let project = tempfile::tempdir_in(&workspace).expect("isolated Godot project");
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[physics]\n3d/physics_engine=\"Jolt Physics\"\n",
    )
    .expect("write project settings");
    std::fs::write(
        project.path().join("soe.gdextension"),
        format!(
            "[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\nlinux.release.x86_64=\"{}\"\n",
            library.display(),
            library.display()
        ),
    )
    .expect("write extension configuration");
    std::fs::create_dir(project.path().join(".godot")).expect("extension list directory");
    std::fs::write(
        project.path().join(".godot/extension_list.cfg"),
        "res://soe.gdextension\n",
    )
    .expect("write extension list");
    std::fs::copy(
        workspace.join("godot/physics_test.gd"),
        project.path().join("physics_test.gd"),
    )
    .expect("copy collision script");
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
