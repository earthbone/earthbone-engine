// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use sha2::Digest;
use soe_esm::{read_plugin, read_plugin_cached, FileHeader, Plugin};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("soe-fmt-29-{name}-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn fixture(author: &str) -> Vec<u8> {
    let header = FileHeader {
        author: author.to_owned(),
        ..FileHeader::default()
    };
    Plugin::new(header).to_bytes()
}

fn cache_files(path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|entry| {
            entry
                .extension()
                .is_some_and(|ext| ext == "soe-plugin-cache")
        })
        .collect()
}

#[test]
fn fmt_29_fixture_cache_hit_matches_fresh_parse_and_uses_canonical_path() {
    let root = temp_dir("canonical");
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    std::fs::write(&source, fixture("fmt-29")).unwrap();
    let fresh = read_plugin(&source).unwrap().unwrap();
    let first = read_plugin_cached(&source, &cache).unwrap().unwrap();
    let second = read_plugin_cached(&source, &cache).unwrap().unwrap();
    assert_eq!(first, fresh);
    assert_eq!(second, fresh);

    #[cfg(unix)]
    {
        let alias = root.join("alias.esp");
        std::os::unix::fs::symlink(&source, &alias).unwrap();
        assert_eq!(read_plugin_cached(alias, &cache).unwrap().unwrap(), fresh);
        assert_eq!(cache_files(&cache).len(), 1);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_29_changed_file_identity_parses_and_writes_a_new_entry() {
    let root = temp_dir("changed");
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    std::fs::write(&source, fixture("before")).unwrap();
    let first = read_plugin_cached(&source, &cache).unwrap().unwrap();
    std::fs::write(&source, fixture("after, with a longer header")).unwrap();
    let fresh = read_plugin(&source).unwrap().unwrap();
    let changed = read_plugin_cached(&source, &cache).unwrap().unwrap();
    assert_ne!(first, fresh);
    assert_eq!(changed, fresh);
    assert_eq!(cache_files(&cache).len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_29_same_size_modified_file_uses_modification_time_key() {
    let root = temp_dir("mtime");
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    std::fs::write(&source, fixture("before")).unwrap();
    let first = read_plugin_cached(&source, &cache).unwrap().unwrap();
    let original_modified = std::fs::metadata(&source).unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&source, fixture("after!")).unwrap();
    let changed_modified = std::fs::metadata(&source).unwrap().modified().unwrap();
    assert_ne!(original_modified, changed_modified);
    let fresh = read_plugin(&source).unwrap().unwrap();
    let changed = read_plugin_cached(&source, &cache).unwrap().unwrap();
    assert_ne!(first, fresh);
    assert_eq!(changed, fresh);
    assert_eq!(cache_files(&cache).len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_29_corrupt_and_truncated_entries_fall_back_and_are_rewritten() {
    let root = temp_dir("damaged");
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    std::fs::write(&source, fixture("recover")).unwrap();
    let fresh = read_plugin(&source).unwrap().unwrap();
    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);
    let entry = cache_files(&cache).pop().unwrap();

    let mut old_version = std::fs::read(&entry).unwrap();
    let path_len = u64::from_le_bytes(old_version[48..56].try_into().unwrap()) as usize;
    let version_offset = 48 + 8 + path_len + 8 + 8 + 4;
    old_version[version_offset..version_offset + 4].copy_from_slice(&0u32.to_le_bytes());
    let digest = sha2::Sha256::digest(&old_version[48..]);
    old_version[16..48].copy_from_slice(&digest);
    let old_version_bytes = old_version.clone();
    std::fs::write(&entry, old_version).unwrap();
    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);
    assert_ne!(std::fs::read(&entry).unwrap(), old_version_bytes);

    let mut corrupt = std::fs::read(&entry).unwrap();
    *corrupt.last_mut().unwrap() ^= 0x80;
    let damaged = corrupt.clone();
    std::fs::write(&entry, corrupt).unwrap();
    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);
    assert_ne!(std::fs::read(&entry).unwrap(), damaged);

    std::fs::write(&entry, b"SOEPC001").unwrap();
    assert_eq!(read_plugin_cached(&source, &cache).unwrap().unwrap(), fresh);
    assert_ne!(std::fs::read(&entry).unwrap(), b"SOEPC001");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fmt_29_installed_skyrim_master_cache_hit_matches_and_times_second_load() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed Skyrim.esm cache check");
        return;
    };
    let root = temp_dir("installed");
    let source = data.join("Skyrim.esm");
    let cache = root.join("cache");

    let first_started = Instant::now();
    let first = read_plugin_cached(&source, &cache)
        .unwrap()
        .unwrap_or_else(|error| panic!("Skyrim.esm: {error}"));
    let first_elapsed = first_started.elapsed();

    let second_started = Instant::now();
    let second = read_plugin_cached(&source, &cache)
        .unwrap()
        .unwrap_or_else(|error| panic!("Skyrim.esm cache load: {error}"));
    let second_elapsed = second_started.elapsed();
    let fresh = read_plugin(&source)
        .unwrap()
        .unwrap_or_else(|error| panic!("fresh Skyrim.esm parse: {error}"));

    assert_eq!(first, fresh);
    assert_eq!(second, fresh);
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let master = data.join(name);
        let cached_first = read_plugin_cached(&master, &cache)
            .unwrap()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let cached_second = read_plugin_cached(&master, &cache)
            .unwrap()
            .unwrap_or_else(|error| panic!("{name} cache load: {error}"));
        let fresh_master = read_plugin(&master)
            .unwrap()
            .unwrap_or_else(|error| panic!("fresh {name} parse: {error}"));
        assert_eq!(cached_first, fresh_master, "first cached load of {name}");
        assert_eq!(cached_second, fresh_master, "cache hit for {name}");
    }
    eprintln!(
        "FMT-29 Skyrim.esm first load: {:.3}s, second cached load: {:.3}s",
        first_elapsed.as_secs_f64(),
        second_elapsed.as_secs_f64()
    );
    std::fs::remove_dir_all(root).unwrap();
}
