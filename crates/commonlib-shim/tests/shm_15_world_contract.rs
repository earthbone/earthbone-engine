// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_15_cpp_world_contract_compiles_and_runs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-15-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("world.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>

struct Fixture final : RE::TES {
    std::optional<RE::Setting> setting(const std::string& name) const override {
        return RE::Setting{name,
            RE::SettingValue{RE::SettingValue::Type::Float, false, 0, 3.5F, {}}};
    }
    RE::Sky sky() const override { return {18.25F}; }
    std::optional<RE::ReanimateEffect> reanimate_effect(std::uint64_t id) const override {
        if (id != 4) return std::nullopt;
        return RE::ReanimateEffect{4, {1}, {2}, {3}, 8.0F, 1.0, 5.0};
    }
    bool sit_sleep_state(RE::FormId, RE::SIT_SLEEP_STATE&,
                         RE::UnsupportedWorldHook& error) const override {
        error = {"SIT_SLEEP_STATE", "runtime state unavailable"};
        return false;
    }
    RE::UnsupportedWorldHook hook(const std::string&) const override {
        return {"binary hook", "no target in this engine"};
    }
};

int main() {
    Fixture world;
    const auto setting = world.setting("fTest");
    assert(setting && setting->name == "fTest" && setting->value.real == 3.5F);
    assert(world.sky().hour == 18.25F);
    assert(world.reanimate_effect(4)->target.value == 3);
    assert(!world.reanimate_effect(5));
    RE::SIT_SLEEP_STATE state{};
    RE::UnsupportedWorldHook error{};
    assert(!world.sit_sleep_state({1}, state, error));
    assert(std::string(error.name) == "SIT_SLEEP_STATE");
    assert(std::string(world.hook("write_vfunc").reason).find("no target") != std::string::npos);
}
"#,
    )
    .unwrap();
    let binary = scratch.join("world-contract");
    let compiled = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-15");
    assert!(
        compiled.status.success(),
        "C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let ran = Command::new(&binary).output().unwrap();
    assert!(ran.status.success(), "C++ fixture did not pass");
}
