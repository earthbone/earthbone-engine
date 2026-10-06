// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn first_save(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "ess") {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn prf_21_breaker_cli_reports_plugin_and_section_times_and_papyrus_script_ownership() {
    let root = std::env::var_os("SOE_TEST_SAVES")
        .map(PathBuf::from)
        .expect("SOE_TEST_SAVES is set for the save survey");
    let path = first_save(&root).expect("the save corpus contains an ESS file");
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["save", path.to_str().unwrap()])
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "soe-bench save failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let missing = [
        (
            text.contains("save_section_save_ns") || text.contains("save_section_load_ns"),
            "per-section save/load timings",
        ),
        (
            text.contains("save_plugin_load_ns"),
            "per-plugin load timings",
        ),
        (
            text.contains("save_papyrus_script_plugin"),
            "Papyrus script owner attribution",
        ),
        (
            text.contains("save_papyrus_script_bytes"),
            "per-script Papyrus byte totals",
        ),
    ]
    .into_iter()
    .filter_map(|(present, name)| (!present).then_some(name))
    .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "missing report fields: {}",
        missing.join(", ")
    );
}
