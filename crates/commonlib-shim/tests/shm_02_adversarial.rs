// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("soe-shm-02-adversarial-{name}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(source: &Path, output: &Path, shared: bool) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new("c++");
    command.arg("-std=c++20").arg("-I").arg(root.join("include"));
    if shared {
        command.args(["-shared", "-fPIC"]);
    } else {
        command.arg("-ldl");
    }
    let result = command.arg(source).arg("-o").arg(output).output().unwrap();
    assert!(
        result.status.success(),
        "C++ compile failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn run_fixture(dir: &Path, source: &str) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cpp = dir.join("plugin.cpp");
    let library = dir.join("plugin.so");
    let loader = dir.join("plugin-loader");
    fs::write(&cpp, source).unwrap();
    compile(&cpp, &library, true);
    compile(&root.join("tools/plugin-loader.cpp"), &loader, false);
    Command::new(loader).arg(library).output().unwrap()
}

#[test]
fn shm_02_rejects_incompatible_manifest_version() {
    let dir = scratch("abi-version");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION + 1, "future", nullptr, 0};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid plugin manifest"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_null_manifest_name() {
    let dir = scratch("null-name");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, nullptr, nullptr, 0};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid plugin manifest"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_missing_hook_array_for_nonzero_count() {
    let dir = scratch("null-hooks");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "bad hooks", nullptr, 1};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid plugin manifest"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_empty_unsupported_hook_name() {
    let dir = scratch("empty-hook");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
static const char* unsupported[] = {""};
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "empty hook", unsupported, 1};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unnamed unsupported hook"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_failed_load_is_not_reported_as_loaded() {
    let dir = scratch("failed-load");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "rejecting plugin", nullptr, 0};
SOE_PLUGIN_LOAD { return false; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("plugin entry point returned false"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("loaded: rejecting plugin"));
    fs::remove_dir_all(dir).unwrap();
}
