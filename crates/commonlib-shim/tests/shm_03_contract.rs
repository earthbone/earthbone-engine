// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_03_cpp_contracts_expose_live_world_state_and_reject_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-03-contract-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contract.cpp");
    let executable = scratch.join("contract");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>

static_assert(static_cast<std::uint32_t>(RE::ACTOR_CRITICAL_STAGE::kNone) == 0);
static_assert(static_cast<std::uint32_t>(RE::ACTOR_CRITICAL_STAGE::kDisintegrateEnd) == 4);
static_assert(static_cast<std::uint32_t>(RE::ACTOR_VALUE_MODIFIER::kPermanent) == 0);
static_assert(static_cast<std::uint32_t>(RE::ACTOR_VALUE_MODIFIER::kTemporary) == 1);
static_assert(static_cast<std::uint32_t>(RE::ACTOR_VALUE_MODIFIER::kDamage) == 2);
static_assert(sizeof(RE::ActivateParentID) == 8);
static_assert(std::same_as<decltype(&RE::WorldState::critical_stage), RE::Result<RE::ACTOR_CRITICAL_STAGE> (RE::WorldState::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::WorldState::set_critical_stage), bool (RE::WorldState::*)(RE::FormId, RE::ACTOR_CRITICAL_STAGE)>);
static_assert(std::same_as<decltype(&RE::WorldState::actor_value_modifier), float (RE::WorldState::*)(RE::FormId, std::uint32_t, RE::ACTOR_VALUE_MODIFIER)>);
static_assert(std::same_as<decltype(&RE::WorldState::ai_process), RE::AIProcess (RE::WorldState::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::WorldState::activate_parents), std::vector<RE::ActivateParentID> (RE::WorldState::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::WorldState::active_effect), std::optional<RE::ActiveEffect> (RE::WorldState::*)(std::uint64_t) const>);
static_assert(std::same_as<decltype(&RE::WorldState::unsupported_hook), RE::UnsupportedState (RE::WorldState::*)(const std::string&) const>);

struct ContractFixture : RE::WorldState {
    RE::Result<RE::ACTOR_CRITICAL_STAGE> critical_stage(RE::FormId actor) override {
        if (actor.value == 0) return {std::nullopt, RE::UnsupportedState{"actor state unavailable"}};
        return {RE::ACTOR_CRITICAL_STAGE::kNone, std::nullopt};
    }
    bool set_critical_stage(RE::FormId, RE::ACTOR_CRITICAL_STAGE) override { return true; }
    float actor_value_modifier(RE::FormId, std::uint32_t, RE::ACTOR_VALUE_MODIFIER) override { return 0.0F; }
    RE::AIProcess ai_process(RE::FormId) override { return {}; }
    std::vector<RE::ActivateParentID> activate_parents(RE::FormId) override { return {}; }
    std::optional<RE::ActiveEffect> active_effect(std::uint64_t) const override { return std::nullopt; }
    RE::UnsupportedState unsupported_hook(const std::string& name) const override { return {name.c_str()}; }
};

int main() {
    ContractFixture fixture;
    const auto present = fixture.critical_stage(RE::FormId{1});
    const auto missing = fixture.critical_stage(RE::FormId{0});
    return present.value == RE::ACTOR_CRITICAL_STAGE::kNone && !present.error &&
        !missing.value && missing.error &&
        std::string(missing.error->message) == "actor state unavailable" ? 0 : 1;
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required for the SHM-03 contract fixture");
    assert!(
        output.status.success(),
        "SHM-03 C++ contract failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "SHM-03 contract fixture failed");
    fs::remove_dir_all(scratch).unwrap();
}
