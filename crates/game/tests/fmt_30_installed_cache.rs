// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[path = "common/installed.rs"]
mod installed;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("soe-fmt-30-game-cache-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn fmt_30_installed_helper_second_binary_child() {
    if std::env::var_os("FMT_30_GAME_CHILD").is_some() {
        let order = installed::load_order(&[
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ])
        .unwrap();
        assert!(order.len() > 1_000_000);
    }
}

#[test]
fn fmt_30_installed_helper_persists_master_entries_for_another_test_binary() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping installed plugin cache check");
        return;
    };
    let cache = temp_dir();
    let previous_cache = std::env::var_os("SOE_PLUGIN_CACHE");
    std::env::set_var("SOE_PLUGIN_CACHE", &cache);
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let order = installed::load_order(&masters).unwrap();
    assert!(order.len() > 1_000_000);
    let entries = || {
        std::fs::read_dir(&cache)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "soe-plugin-cache")
            })
            .count()
    };
    assert_eq!(entries(), masters.len());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("fmt_30_installed_cache::fmt_30_installed_helper_second_binary_child")
        .arg("--nocapture")
        .env("FMT_30_GAME_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(path) = previous_cache {
        std::env::set_var("SOE_PLUGIN_CACHE", path);
    } else {
        std::env::remove_var("SOE_PLUGIN_CACHE");
    }
    std::fs::remove_dir_all(cache).unwrap();
}
