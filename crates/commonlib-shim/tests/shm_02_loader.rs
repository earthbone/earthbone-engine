// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("soe-shm-02-{name}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(source: &Path, output: &Path, shared: bool) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new("c++");
    command
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"));
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

fn build_loader(dir: &Path) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let loader = dir.join("plugin-loader");
    compile(&root.join("tools/plugin-loader.cpp"), &loader, false);
    loader
}

fn run_fixture(dir: &Path, source: &str) -> Output {
    let cpp = dir.join("plugin.cpp");
    let library = dir.join("plugin.so");
    fs::write(&cpp, source).unwrap();
    compile(&cpp, &library, true);
    Command::new(build_loader(dir))
        .arg(library)
        .output()
        .unwrap()
}

#[test]
fn shm_02_compiled_module_loads_and_reports_unsupported_hooks() {
    let dir = scratch("loads");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/SKSE.h>
static const char* unsupported[] = {
    "Papyrus Extender detection call patch",
    "Papyrus Extender fast travel vtable patch"
};
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "shim fixture", unsupported, 2};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(
        result.status.success(),
        "loader failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("unsupported hook: Papyrus Extender detection call patch"));
    assert!(stdout.contains("unsupported hook: Papyrus Extender fast travel vtable patch"));
    assert!(stdout.contains("loaded: shim fixture"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_module_without_load_entry_point() {
    let dir = scratch("missing-entry");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "incomplete", nullptr, 0};
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("missing SOEPlugin_Manifest or SOEPlugin_Load"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_module_with_unlisted_hook() {
    let dir = scratch("unnamed-hook");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
static const char* unsupported[] = {nullptr};
SOE_PLUGIN_MANIFEST = {SKSE::SOE_PLUGIN_ABI_VERSION, "incomplete", unsupported, 1};
SOE_PLUGIN_LOAD { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unnamed unsupported hook"));
    fs::remove_dir_all(dir).unwrap();
}
