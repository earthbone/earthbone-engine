// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use soe_godot_host::data_source::DataSource;

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("soe-fmt-30-host-cache-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn cache_entries(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
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
fn fmt_30_host_user_cache_and_off_switch() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed host plugin cache check");
        return;
    };
    let cache_root = temp_dir();
    let cache = cache_root.join("skyrim-open-engine").join("plugins");
    let previous_cache = std::env::var_os("SOE_PLUGIN_CACHE");
    let previous_xdg = std::env::var_os("XDG_CACHE_HOME");

    std::env::remove_var("SOE_PLUGIN_CACHE");
    std::env::set_var("XDG_CACHE_HOME", &cache_root);
    let source = DataSource::open_with_threads(&data, 1).expect("open with user cache enabled");
    drop(source);
    assert_eq!(cache_entries(&cache), 5);

    std::fs::remove_dir_all(&cache_root).unwrap();
    std::env::set_var("SOE_PLUGIN_CACHE", "off");
    let source = DataSource::open_with_threads(&data, 1).expect("open with cache disabled");
    drop(source);
    assert!(!cache_root.exists());

    if let Some(path) = previous_cache {
        std::env::set_var("SOE_PLUGIN_CACHE", path);
    } else {
        std::env::remove_var("SOE_PLUGIN_CACHE");
    }
    if let Some(path) = previous_xdg {
        std::env::set_var("XDG_CACHE_HOME", path);
    } else {
        std::env::remove_var("XDG_CACHE_HOME");
    }
}
