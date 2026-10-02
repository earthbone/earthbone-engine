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

const PAPYRUS_EXTENDER_ADAPTATION: &str = r#"
#include <SKSE/SKSE.h>

SKSE_PLUGIN_VERSION = {1, 100, "Papyrus Extender source adaptation"};
static const char* unsupportedHooks[] = {
    "Papyrus Extender trampoline patches",
    "Papyrus Extender scene graph and Havok natives"
};
SKSE_PLUGIN_UNSUPPORTED_HOOKS = {unsupportedHooks, 2};

SKSE_PLUGIN_QUERY(SKSE::QueryInterface*, SKSE::PluginInfo* info)
{
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "Papyrus Extender source adaptation";
    info->version = 100;
    return true;
}

SKSE_PLUGIN_LOAD(const SKSE::LoadInterface*)
{
    return true;
}
"#;

#[test]
fn shm_02_papyrus_extender_adaptation_compiles_loads_and_reports_unsupported_hooks() {
    let dir = scratch("papyrus-extender");
    let result = run_fixture(&dir, PAPYRUS_EXTENDER_ADAPTATION);
    assert!(
        result.status.success(),
        "loader failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("plugin: Papyrus Extender source adaptation v100"));
    assert!(stdout.contains("unsupported hook: Papyrus Extender trampoline patches"));
    assert!(stdout.contains("unsupported hook: Papyrus Extender scene graph and Havok natives"));
    assert!(stdout.contains("loaded: Papyrus Extender source adaptation"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_plugin_missing_a_standard_skse_export() {
    let dir = scratch("missing-export");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "missing query"};
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface*) { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("missing SKSEPlugin_Query, SKSEPlugin_Load or SKSEPlugin_Version"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_plugin_that_declines_the_runtime_in_query() {
    let dir = scratch("query-failure");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "query failure"};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface*, SKSE::PluginInfo*) { return false; }
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface*) { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("plugin query rejected this runtime"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_rejects_plugin_that_fails_to_load() {
    let dir = scratch("load-failure");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "load failure"};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface*, SKSE::PluginInfo* info)
{
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "load failure";
    info->version = 1;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface*) { return false; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("plugin entry point returned false"));
    fs::remove_dir_all(dir).unwrap();
}
