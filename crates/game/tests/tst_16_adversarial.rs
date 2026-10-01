// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for the TST-16 integration-test harness contract.

use std::path::Path;

fn is_registered(source: &str, name: &str) -> bool {
    let lines: Vec<_> = source.lines().map(str::trim).collect();
    let path = format!("#[path = \"{name}.rs\"]");
    let module = format!("mod {name};");
    lines
        .windows(2)
        .any(|pair| pair[0] == path && pair[1] == module)
}

#[test]
fn tst_16_registration_must_be_an_adjacent_active_declaration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let source = std::fs::read_to_string(root.join("it.rs")).unwrap();
    let own_module = "tst_16_adversarial";
    assert!(is_registered(&source, own_module));
    assert!(!is_registered(
        "// #[path = \"hidden.rs\"]\n// mod hidden;",
        "hidden"
    ));
    assert!(!is_registered(
        "#[path = \"wrong.rs\"]\nmod other;",
        "wrong"
    ));
}

#[test]
fn tst_16_performance_record_contains_before_and_after_measurements() {
    let performance = include_str!("../../../docs/performance.md");
    let section = performance
        .split("TST-16")
        .nth(1)
        .expect("docs/performance.md must record the TST-16 test-binary measurement");
    assert!(section.contains("before") && section.contains("after"));
    assert!(section.contains("--no-run"));
    assert!(section.contains("binary"));
}
