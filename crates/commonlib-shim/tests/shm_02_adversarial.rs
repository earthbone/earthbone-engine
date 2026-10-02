// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "soe-shm-02-adversarial-{name}-{}",
        std::process::id()
    ));
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
    let compile_plugin = compile(&cpp, &library, true);
    assert!(
        compile_plugin.status.success(),
        "plugin compile failed: {}",
        String::from_utf8_lossy(&compile_plugin.stderr)
    );
    let compile_loader = compile(&root.join("tools/plugin-loader.cpp"), &loader, false);
    assert!(
        compile_loader.status.success(),
        "loader compile failed: {}",
        String::from_utf8_lossy(&compile_loader.stderr)
    );
    Command::new(loader).arg(library).output().unwrap()
}

#[test]
fn shm_02_plugin_version_data_matches_commonlibsse_layout() {
    let dir = scratch("version-layout");
    let cpp = dir.join("layout.cpp");
    let output = dir.join("layout");
    fs::write(
        &cpp,
        r"
#include <SKSE/Plugin.h>
#include <cstddef>
static_assert(offsetof(SKSE::PluginVersionData, pluginName) == 0x008);
static_assert(offsetof(SKSE::PluginVersionData, author) == 0x108);
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
fn shm_02_loader_supplies_interfaces_needed_by_surveyed_plugin_query() {
    let dir = scratch("query-interface");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "Papyrus Extender"};
SKSE_PLUGIN_QUERY(const SKSE::QueryInterface* skse, SKSE::PluginInfo* info)
{
    if (!skse) return false;
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "powerofthree's Papyrus Extender";
    info->version = 100;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface* skse) { return skse != nullptr; }
"#,
    );
    assert!(
        result.status.success(),
        "surveyed plugin startup rejected: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shm_02_plugin_version_data_supports_commonlib_builder_api() {
    let dir = scratch("version-builder");
    let cpp = dir.join("builder.cpp");
    let output = dir.join("builder");
    fs::write(
        &cpp,
        r#"
#include <SKSE/Plugin.h>
int main()
{
    SKSE::PluginVersionData version;
    version.PluginVersion(100);
    version.PluginName("powerofthree's Papyrus Extender");
    version.AuthorName("powerofthree");
    version.CompatibleVersions({1});
}
"#,
    )
    .unwrap();
    let result = compile(&cpp, &output, false);
    assert!(
        result.status.success(),
        "CommonLibSSE version API compile failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}
