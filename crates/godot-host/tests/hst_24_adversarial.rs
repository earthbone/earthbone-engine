// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-24 pause input and overlay teardown adversarial coverage.

#[test]
fn hst_24_escape_runtime_acceptance() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(root.join("godot"))
        .args(["--script", "hst_24_pause_test.gd"])
        .env("SOE_HST_24_MODE", "quit")
        .env("SOE_HST_24_INSTALLED", "0")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "pause check exit {:?}: {log}",
        output.status.code()
    );
    assert!(log.contains("hst_24_escape_toggles_pause: PASS"), "{log}");
    assert!(!log.contains("SCRIPT ERROR:"), "{log}");
}

#[test]
fn hst_24_escape_translation_matches_godot_key_value() {
    use soe_godot_host::input::{godot_key, godot_key_for, scan};
    assert_eq!(godot_key_for(scan::ESCAPE), Some(godot_key::ESCAPE));
    assert_eq!(godot_key_for(0), None);
}
