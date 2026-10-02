// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for shared TST-16 test helpers.

use std::path::{Path, PathBuf};

fn rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn declared_modules(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let declaration = line.trim().strip_prefix("pub(crate) ").unwrap_or(line.trim());
            declaration
                .strip_prefix("mod ")
                .and_then(|name| name.strip_suffix(';'))
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn tst_16_shared_helper_modules_are_declared_only_by_their_owners() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let owners = [root.join("common/mod.rs"), root.join("support/mod.rs")];
    let helpers: Vec<_> = ["common", "support"]
        .into_iter()
        .map(str::to_owned)
        .chain(owners.iter().flat_map(|path| declared_modules(path)))
        .collect();
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    for path in files {
        if owners.contains(&path) || path == root.join("it.rs") {
            continue;
        }
        let duplicate: Vec<_> = declared_modules(&path)
            .into_iter()
            .filter(|name| helpers.contains(name))
            .collect();
        assert!(
            duplicate.is_empty(),
            "{} redeclares shared helpers {duplicate:?}; import through crate::",
            path.display()
        );
    }
}

#[test]
fn tst_16_registration_guard_does_not_accept_a_commented_out_module() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tst_16_test_binary.rs"),
    )
    .unwrap();
    assert!(
        !source.contains(".contains(&registration(name))"),
        "substring matching accepts a commented-out module declaration"
    );
}
