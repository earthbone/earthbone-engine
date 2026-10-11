// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "soe-shm-02-breaker-3-{name}-{}",
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
fn native_registry_keeps_first_case_insensitive_registration_and_metadata() {
    let dir = scratch("registry-first-wins");
    let cpp = dir.join("registry.cpp");
    let output = dir.join("registry");
    fs::write(
        &cpp,
        r#"
#include <RE/VirtualMachine.h>
#include <cassert>
int main()
{
    RE::NativeRegistry registry;
    assert(registry.register_native({"PO3_SKSEFunctions", "GetFormEditorID", true}));
    assert(!registry.register_native({"po3_sksefunctions", "getformeditorid", false}));
    assert(registry.len() == 1);
    assert(registry.duplicates().size() == 1);
    const auto* first = registry.get("Po3_SkseFunctions", "GETFORMEDITORID");
    assert(first && first->latent);
    assert(first->class_name == "PO3_SKSEFunctions");
    assert(registry.get("PO3_SKSEFunctions", "Missing") == nullptr);
}
"#,
    )
    .unwrap();
    let result = compile(&cpp, &output, false);
    assert!(
        result.status.success(),
        "registry compile failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let run = Command::new(output).output().unwrap();
    assert!(run.status.success());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn loader_reports_duplicate_registration_when_callback_accepts_registry() {
    let dir = scratch("duplicate-registration");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/SKSE.h>
SKSE_PLUGIN_VERSION = {1, 1, "duplicate registration"};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface*, SKSE::PluginInfo* info)
{
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "duplicate registration";
    info->version = 1;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface* skse)
{
    auto* papyrus = static_cast<SKSE::PapyrusInterface*>(skse->queryInterface(2));
    if (!papyrus) return false;
    papyrus->register_natives([](RE::NativeRegistry& registry) {
        registry.register_native({"Form", "GetFormEditorID", false});
        registry.register_native({"form", "getformeditorid", false});
        return true;
    });
    return true;
}
"#,
    );
    assert!(
        result.status.success(),
        "loader rejected accepted registration: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("registered natives: 1"));
    assert!(stdout.contains("duplicate natives: 1"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn loader_rejects_unnamed_unsupported_hook_instead_of_silently_loading() {
    let dir = scratch("unnamed-hook");
    let result = run_fixture(
        &dir,
        r#"
#include <SKSE/Plugin.h>
static const char* hooks[] = {"supported-name", ""};
SKSE_PLUGIN_VERSION = {1, 1, "unnamed hook"};
SKSE_PLUGIN_UNSUPPORTED_HOOKS = {hooks, 2};
SKSE_PLUGIN_QUERY(SKSE::QueryInterface*, SKSE::PluginInfo* info)
{
    info->infoVersion = SKSE::PluginInfo::kVersion;
    info->name = "unnamed hook";
    info->version = 1;
    return true;
}
SKSE_PLUGIN_LOAD(const SKSE::LoadInterface*) { return true; }
"#,
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unnamed unsupported hook"));
    fs::remove_dir_all(dir).unwrap();
}
