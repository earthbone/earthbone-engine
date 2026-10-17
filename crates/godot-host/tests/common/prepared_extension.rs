// SPDX-License-Identifier: GPL-3.0-or-later

//! Host fixtures reuse a current release library prepared outside Cargo tests.
//! Prepare it once through the shared build wrapper instead of inside each fixture.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn newest(directory: &Path) -> SystemTime {
    std::fs::read_dir(directory)
        .expect("read extension source directory")
        .map(|entry| {
            let path = entry.expect("read extension source entry").path();
            if path.is_dir() {
                newest(&path)
            } else {
                modified(&path)
            }
        })
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .expect("read extension input metadata")
        .modified()
        .expect("read extension input modification time")
}

/// Require a release library newer than its manifests and production sources.
///
/// # Panics
/// Fails when the prepared library is missing, stale or cannot be inspected.
pub fn require(workspace: &Path) -> PathBuf {
    let executable = std::env::current_exe().expect("test executable path");
    let target = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("Cargo target directory");
    let library = target.join("release").join(format!(
        "{}soe_godot_host{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    assert!(
        library.is_file(),
        "missing prepared extension {}. Build the release extension before cargo test",
        library.display()
    );
    let built = modified(&library);
    let mut inputs =
        modified(&workspace.join("Cargo.toml")).max(modified(&workspace.join("Cargo.lock")));
    for entry in std::fs::read_dir(workspace.join("crates")).expect("read workspace crates") {
        let directory = entry.expect("read workspace crate").path();
        for name in ["Cargo.toml", "build.rs"] {
            let path = directory.join(name);
            if path.is_file() {
                inputs = inputs.max(modified(&path));
            }
        }
        let source = directory.join("src");
        if source.is_dir() {
            inputs = inputs.max(newest(&source));
        }
    }
    assert!(
        built >= inputs,
        "stale prepared extension {}. Rebuild the release extension before cargo test",
        library.display()
    );
    library
}
