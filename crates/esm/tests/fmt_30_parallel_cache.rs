// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use soe_esm::{read_plugin, read_plugin_cached, FileHeader, Plugin};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("soe-fmt-30-parallel-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn fmt_30_parallel_writer_child() {
    let Some(root) = std::env::var_os("FMT_30_ROOT").map(PathBuf::from) else {
        return;
    };
    let id = std::env::var("FMT_30_WRITER_ID").unwrap();
    let ready = root.join(format!("ready-{id}"));
    std::fs::write(&ready, b"ready").unwrap();
    let gate = root.join("start");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !gate.exists() {
        assert!(Instant::now() < deadline, "parallel writer start timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    let source = root.join("fixture.esp");
    let cache = root.join("cache");
    let cached = read_plugin_cached(&source, &cache).unwrap().unwrap();
    let fresh = read_plugin(&source).unwrap().unwrap();
    assert_eq!(cached, fresh);
}

#[test]
fn fmt_30_parallel_process_writers_leave_a_complete_cache_entry() {
    let root = temp_dir();
    let source = root.join("fixture.esp");
    let plugin = Plugin::new(FileHeader {
        author: "fmt-30 concurrent writers".to_owned(),
        ..FileHeader::default()
    });
    std::fs::write(&source, plugin.to_bytes()).unwrap();

    let executable = std::env::current_exe().unwrap();
    let mut children: Vec<Child> = (0..6)
        .map(|id| {
            Command::new(&executable)
                .arg("--exact")
                .arg("fmt_30_parallel_writer_child")
                .arg("--nocapture")
                .env("FMT_30_ROOT", &root)
                .env("FMT_30_WRITER_ID", id.to_string())
                .spawn()
                .unwrap()
        })
        .collect();

    let deadline = Instant::now() + Duration::from_secs(10);
    while (0..6).any(|id| !root.join(format!("ready-{id}")).exists()) {
        assert!(
            Instant::now() < deadline,
            "parallel writers did not become ready"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    std::fs::write(root.join("start"), b"go").unwrap();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }

    let cache = root.join("cache");
    let entries: Vec<_> = std::fs::read_dir(&cache)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "soe-plugin-cache")
        })
        .collect();
    assert_eq!(entries.len(), 1);
    let recovered = read_plugin_cached(&source, &cache).unwrap().unwrap();
    assert_eq!(recovered, read_plugin(&source).unwrap().unwrap());
    let bytes = std::fs::read(&entries[0]).unwrap();
    assert!(bytes.starts_with(b"SOEPC001"));
    std::fs::remove_dir_all(root).unwrap();
}
