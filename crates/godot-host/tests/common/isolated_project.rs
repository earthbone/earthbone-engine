// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Isolated script checks follow the PRF-14 test's explicit extension registration.
//! Scripts are copied with every `res://` file they load, so a new preload in
//! walkthrough.gd reaches the isolated project without editing each caller.

use std::path::Path;

pub fn project(scripts: &[&str], library: Option<&Path>) -> tempfile::TempDir {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = tempfile::tempdir_in(&workspace).expect("isolated Godot project");
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[physics]\n3d/physics_engine=\"Jolt Physics\"\n",
    )
    .expect("project settings");
    let mut pending: Vec<String> = scripts.iter().map(|script| (*script).to_owned()).collect();
    let mut copied = std::collections::BTreeSet::new();
    while let Some(script) = pending.pop() {
        if !copied.insert(script.clone()) {
            continue;
        }
        let source = workspace.join("godot").join(&script);
        let destination = project.path().join(&script);
        std::fs::create_dir_all(destination.parent().expect("script directory"))
            .expect("create script directory");
        std::fs::copy(&source, destination).expect("copy script");
        let text = std::fs::read_to_string(&source).unwrap_or_default();
        pending
            .extend(res_paths(&text).filter(|path| workspace.join("godot").join(path).is_file()));
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

/// The `res://` paths a Godot script or shader names in quotes.
fn res_paths(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split("\"res://")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
}
