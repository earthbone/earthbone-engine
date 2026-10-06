// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "soe-shm-02-interface-breaker-{name}-{}",
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

#[test]
fn surveyed_papyrus_extender_startup_requires_a_papyrus_interface() {
    let dir = scratch("papyrus-interface");
    let cpp = dir.join("plugin.cpp");
    let library = dir.join("plugin.so");
    let loader = dir.join("plugin-loader");
    fs::write(
        &cpp,
        r#"
#include <SKSE/Plugin.h>
SKSE_PLUGIN_VERSION = {1, 1, "Papyrus Extender adaptation"};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface* skse, SKSE::PluginInfo* info)
{
    if (!skse || !skse->queryInterface) return false;
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "Papyrus Extender adaptation";
    info->version = 1;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface* skse)
{
    return skse && skse->queryInterface && skse->queryInterface(2);
}
"#,
    )
    .unwrap();
    let plugin = compile(&cpp, &library, true);
    assert!(
        plugin.status.success(),
        "{}",
        String::from_utf8_lossy(&plugin.stderr)
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let loader_build = compile(&root.join("tools/plugin-loader.cpp"), &loader, false);
    assert!(
        loader_build.status.success(),
        "{}",
        String::from_utf8_lossy(&loader_build.stderr)
    );
    let result = Command::new(&loader).arg(&library).output().unwrap();
    assert!(
        result.status.success(),
        "Papyrus Extender calls GetPapyrusInterface during load: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}
