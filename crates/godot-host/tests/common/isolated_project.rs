// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Isolated script checks follow the PRF-14 test's explicit extension registration.

use std::path::Path;

pub fn project(scripts: &[&str], library: Option<&Path>) -> tempfile::TempDir {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = tempfile::tempdir_in(&workspace).expect("isolated Godot project");
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[physics]\n3d/physics_engine=\"Jolt Physics\"\n",
    )
    .expect("project settings");
    for script in scripts {
        let destination = project.path().join(script);
        std::fs::create_dir_all(destination.parent().expect("script directory"))
            .expect("create script directory");
        std::fs::copy(workspace.join("godot").join(script), destination).expect("copy script");
    }
    if let Some(library) = library {
        assert!(library.is_file(), "missing extension {}", library.display());
        std::fs::write(
            project.path().join("soe.gdextension"),
            format!(
                "[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\nlinux.release.x86_64=\"{}\"\n",
                library.display(),
                library.display()
            ),
        )
        .expect("extension configuration");
        std::fs::create_dir(project.path().join(".godot")).expect("extension list directory");
        std::fs::write(
            project.path().join(".godot/extension_list.cfg"),
            "res://soe.gdextension\n",
        )
        .expect("extension list");
    }
    project
}
