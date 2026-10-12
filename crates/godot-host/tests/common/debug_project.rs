// SPDX-License-Identifier: GPL-3.0-or-later

//! Scene fixtures use the extension built beside the current debug test binary.

#[path = "isolated_project.rs"]
mod isolated_project;

pub fn project(scripts: &[&str]) -> std::io::Result<tempfile::TempDir> {
    let executable = std::env::current_exe()?;
    let library = executable.with_file_name("libsoe_godot_host.so");
    Ok(isolated_project::project(scripts, Some(&library)))
}
