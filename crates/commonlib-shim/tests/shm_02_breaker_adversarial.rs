// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("soe-shm-02-breaker-{name}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn compile(source: &Path, output: &Path, shared: bool) -> Output {
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
    command.arg(source).arg("-o").arg(output).output().unwrap()
}

fn run_fixture(dir: &Path, source: &str) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cpp = dir.join("plugin.cpp");
    let library = dir.join("plugin.so");
    let loader = dir.join("plugin-loader");
    fs::write(&cpp, source).unwrap();
    let plugin = compile(&cpp, &library, true);
    assert!(
        plugin.status.success(),
        "plugin compile failed: {}",
        String::from_utf8_lossy(&plugin.stderr)
    );
    let loader_build = compile(&root.join("tools/plugin-loader.cpp"), &loader, false);
    assert!(
        loader_build.status.success(),
        "loader compile failed: {}",
        String::from_utf8_lossy(&loader_build.stderr)
    );
    Command::new(loader).arg(library).output().unwrap()
}

#[test]
fn shm_02_full_plugin_version_data_layout_matches_commonlibsse() {
    let dir = scratch("version-layout");
    let cpp = dir.join("layout.cpp");
    let output = dir.join("layout");
    fs::write(
        &cpp,
        r"
#include <SKSE/Plugin.h>
#include <cstddef>
static_assert(offsetof(SKSE::PluginVersionData, author) == 0x108);
static_assert(offsetof(SKSE::PluginVersionData, supportEmail) == 0x208);
static_assert(offsetof(SKSE::PluginVersionData, padding2) == 0x305);
static_assert(offsetof(SKSE::PluginVersionData, padding5) == 0x309);
static_assert(offsetof(SKSE::PluginVersionData, padding6) == 0x30A);
static_assert(offsetof(SKSE::PluginVersionData, compatibleVersions) == 0x30C);
static_assert(offsetof(SKSE::PluginVersionData, xseMinimum) == 0x34C);
static_assert(sizeof(SKSE::PluginVersionData) == 0x350);
int main() {}
",
    )
    .unwrap();
    let result = compile(&cpp, &output, false);
    assert!(
        result.status.success(),
        "CommonLibSSE layout compile failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_loader_passes_non_null_interfaces_to_plugin_startup() {
    let dir = scratch("startup-interfaces");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "Papyrus Extender"};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface* skse, SKSE::PluginInfo* info)
{
    if (!skse || skse->runtimeVersion == 0 || !skse->queryInterface) return false;
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "Papyrus Extender";
    info->version = 1;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface* skse)
{
    return skse && skse->runtimeVersion != 0 && skse->queryInterface;
}
"#,
    );
    assert!(
        result.status.success(),
        "loader rejected standard plugin startup: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}
