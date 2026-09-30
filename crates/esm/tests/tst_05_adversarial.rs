// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn deny_and_audit_are_both_gate_steps_and_the_exception_is_reasoned() {
    let root = root();
    let gate = fs::read_to_string(root.join("tools/lint.sh")).unwrap();
    assert!(gate.contains("step \"cargo deny check\"\ncargo deny check"));
    assert!(gate.contains("step \"cargo audit\"\ncargo audit"));

    let config = fs::read_to_string(root.join("deny.toml")).unwrap();
    assert!(config.contains("RUSTSEC-2024-0436"));
    assert!(config.contains("no safe upgrade available"));
    assert!(config.contains("RUSTSEC-2025-0141"));
    assert!(config.contains("bincode 1.3.3 is a transitive dev dependency of iai-callgrind"));
    assert!(config.contains("RUSTSEC-2026-0173"));
    assert!(config.contains("transitive dev dependency of iai-callgrind"));
}

#[test]
fn ci_installs_locked_versions_of_both_gate_tools() {
    let workflow = fs::read_to_string(root().join(".github/workflows/lint.yml")).unwrap();
    assert!(workflow.contains("cargo install --locked cargo-deny --version 0.20.2"));
    assert!(workflow.contains("cargo install --locked cargo-audit --version 0.22.2"));
}
