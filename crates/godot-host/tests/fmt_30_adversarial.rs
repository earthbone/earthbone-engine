// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use soe_godot_host::data_source::DataSource;

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "soe-fmt-30-host-adversarial-{}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn cache_entries(path: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "soe-plugin-cache")
        })
        .count()
}

#[test]
fn fmt_30_explicit_cache_precedence_and_both_disable_paths() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed host cache checks");
        return;
    };
    let root = temp_dir();
    let xdg = root.join("xdg");
    let selected = root.join("selected");
    let previous_cache = std::env::var_os("SOE_PLUGIN_CACHE");
    let previous_xdg = std::env::var_os("XDG_CACHE_HOME");

    std::env::set_var("XDG_CACHE_HOME", &xdg);
    std::env::set_var("SOE_PLUGIN_CACHE", &selected);
    drop(DataSource::open_with_threads(&data, 1).expect("open with explicit cache"));
    assert_eq!(cache_entries(&selected), 5);
    assert_eq!(
        cache_entries(&xdg.join("skyrim-open-engine").join("plugins")),
        0
    );

    let off_root = root.join("off");
    std::env::set_var("XDG_CACHE_HOME", &off_root);
    std::env::set_var("SOE_PLUGIN_CACHE", "off");
    drop(DataSource::open_with_threads(&data, 1).expect("open with env cache disabled"));
    assert!(!off_root.exists());

    let explicit_root = root.join("explicit-none");
    std::env::set_var("SOE_PLUGIN_CACHE", &explicit_root);
    drop(
        DataSource::open_with_threads_and_cache(&data, 1, None)
            .expect("open with explicit cache disabled"),
    );
    assert!(!explicit_root.exists());

    if let Some(value) = previous_cache {
        std::env::set_var("SOE_PLUGIN_CACHE", value);
    } else {
        std::env::remove_var("SOE_PLUGIN_CACHE");
    }
    if let Some(value) = previous_xdg {
        std::env::set_var("XDG_CACHE_HOME", value);
    } else {
        std::env::remove_var("XDG_CACHE_HOME");
    }
    std::fs::remove_dir_all(root).unwrap();
}
