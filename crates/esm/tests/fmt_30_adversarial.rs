// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::Digest;
use soe_esm::{read_plugin, read_plugin_cached, FileHeader, Plugin};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "soe-fmt-30-adversarial-{}-{id}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn cache_entry(path: &std::path::Path) -> PathBuf {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|entry| {
            entry
                .extension()
                .is_some_and(|extension| extension == "soe-plugin-cache")
        })
        .unwrap()
}

#[test]
fn fmt_30_checksum_valid_cache_with_trailing_payload_is_reparsed_and_replaced() {
    let root = temp_dir("trailing");
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    let plugin = Plugin::new(FileHeader {
        author: "fmt-30 trailing payload".to_owned(),
        ..FileHeader::default()
    });
    std::fs::write(&source, plugin.to_bytes()).unwrap();
    let fresh = read_plugin(&source).unwrap().unwrap();
    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);

    let entry = cache_entry(&cache);
    let mut bytes = std::fs::read(&entry).unwrap();
    let payload_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    bytes.extend_from_slice(b"trailing");
    bytes[8..16].copy_from_slice(&(payload_len + 8).to_le_bytes());
    let digest = sha2::Sha256::digest(&bytes[48..]);
    bytes[16..48].copy_from_slice(&digest);
    let damaged = bytes.clone();
    std::fs::write(&entry, bytes).unwrap();

    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);
    assert_ne!(std::fs::read(&entry).unwrap(), damaged);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_30_unwritable_cache_directory_does_not_prevent_source_parse() {
    let root = temp_dir("unwritable");
    let source = root.join("fixture.esp");
    let cache = root.join("cache-is-a-file");
    let plugin = Plugin::new(FileHeader {
        author: "fmt-30 cache failure".to_owned(),
        ..FileHeader::default()
    });
    std::fs::write(&source, plugin.to_bytes()).unwrap();
    std::fs::write(&cache, b"not a directory").unwrap();

    let expected = read_plugin(&source).unwrap().unwrap();
    assert_eq!(
        read_plugin_cached(&source, &cache).unwrap().unwrap(),
        expected
    );
    std::fs::remove_dir_all(root).unwrap();
}
