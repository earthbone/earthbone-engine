// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn native_save_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let data = PathBuf::from(std::env::var_os("SKYRIM_DATA").expect("SKYRIM_DATA is set"));
        let mut order = soe_esm::LoadOrder::new();
        for name in [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ] {
            let plugin = soe_esm::read_plugin(data.join(name))
                .unwrap()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            order.add(name, plugin).unwrap();
        }
        let game = soe_game::Game::new(order);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("prf-21-native-{}.soeg", std::process::id()));
        std::fs::write(&path, game.save_state().unwrap()).unwrap();
        path
    })
    .as_path()
}

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
    let Some(root) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("SOE_TEST_SAVES unset, skipping PRF-21 save report check");
        return;
    };
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

#[test]
fn prf_21_breaker_native_cli_reports_load_time_for_each_named_section() {
    let path = native_save_path();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["--runs", "1", "save", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "soe-bench native save failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let missing_sections = ["world", "papyrus_vm", "envelope"]
        .into_iter()
        .filter(|section| {
            !text
                .lines()
                .any(|line| line.starts_with(&format!("save_section_load_ns {section} ")))
        })
        .collect::<Vec<_>>();
    assert!(
        missing_sections.is_empty(),
        "native report omits load timing for {missing_sections:?}"
    );
}

#[test]
fn prf_21_breaker_native_section_sizes_partition_the_file() {
    let path = native_save_path();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["--runs", "1", "save", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "soe-bench native save failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let section_bytes: usize = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == "save_section")
                .then(|| fields.nth(1)?.parse::<usize>().ok())
                .flatten()
        })
        .sum();
    let file_bytes = text
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next()? == "save_file_bytes")
                .then(|| fields.next()?.parse::<usize>().ok())
                .flatten()
        })
        .expect("native report includes the input file size");
    assert_eq!(
        section_bytes, file_bytes,
        "native named sections must partition the input save"
    );
    for section in ["world", "papyrus_vm", "envelope"] {
        assert!(
            text.lines()
                .any(|line| line.starts_with(&format!("save_section_save_ns {section} "))),
            "native report omits write time for {section}"
        );
    }
}

#[test]
fn prf_21_breaker_native_cli_does_not_fabricate_plugin_or_papyrus_attribution() {
    let path = native_save_path();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-bench"))
        .args(["--runs", "1", "save", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "soe-bench native save failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        !text.lines().any(|line| {
            [
                "save_plugin_section ",
                "save_plugin_load ",
                "save_papyrus_script ",
                "save_papyrus_script_plugin ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        }),
        "an empty native save must not invent plugin or Papyrus attribution"
    );
}
