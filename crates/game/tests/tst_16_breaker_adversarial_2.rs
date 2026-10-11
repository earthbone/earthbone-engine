// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for the consolidated TST-16 test executable.

#[test]
fn tst_16_test_is_running_in_the_it_executable() {
    let executable = std::env::current_exe().unwrap();
    let name = executable.file_stem().unwrap().to_string_lossy();
    assert!(
        name == "it" || name.starts_with("it-"),
        "integration test ran from {name}, expected the consolidated it executable"
    );
}

#[test]
fn tst_16_nested_test_name_keeps_every_module_after_the_binary_prefix() {
    assert_eq!(
        crate::common::child_test_name(
            "it::tst_16_breaker_adversarial_2::nested::scenario",
            "case",
        ),
        "tst_16_breaker_adversarial_2::nested::scenario::case"
    );
}
