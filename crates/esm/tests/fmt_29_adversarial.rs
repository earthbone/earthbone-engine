// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use soe_esm::{read_plugin, read_plugin_cached, FileHeader, Plugin};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "soe-fmt-29-adversarial-{name}-{}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn fixture(author: &str) -> Vec<u8> {
    Plugin::new(FileHeader {
        author: author.to_owned(),
        ..FileHeader::default()
    })
    .to_bytes()
}

#[test]
fn fmt_29_cache_write_failure_does_not_prevent_repeated_source_parses() {
    let root = temp_dir("write-failure");
    let source = root.join("fixture.esp");
    let cache_blocker = root.join("cache-is-a-file");
    std::fs::write(&source, fixture("write fallback")).unwrap();
    std::fs::write(&cache_blocker, b"block directory creation").unwrap();
    let cache_dir = cache_blocker.join("nested");
    let fresh = read_plugin(&source).unwrap().unwrap();

    assert_eq!(
        read_plugin_cached(&source, &cache_dir).unwrap().unwrap(),
        fresh
    );
    assert_eq!(
        read_plugin_cached(&source, &cache_dir).unwrap().unwrap(),
        fresh
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_29_invalid_source_is_returned_without_creating_a_cache_entry() {
    let root = temp_dir("invalid-source");
    let source = root.join("invalid.esp");
    let cache_dir = root.join("cache");
    std::fs::write(&source, b"not a plugin").unwrap();

    assert!(read_plugin_cached(&source, &cache_dir).unwrap().is_err());
    assert!(!cache_dir.exists());
    assert!(read_plugin_cached(&source, &cache_dir).unwrap().is_err());
    assert!(!cache_dir.exists());
    std::fs::remove_dir_all(root).unwrap();
}
