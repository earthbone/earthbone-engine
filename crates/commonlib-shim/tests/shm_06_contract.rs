// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_06_cpp_world_contract_exposes_state_and_named_unsupported_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-06-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contract.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>
static_assert(std::same_as<decltype(&RE::BGSSaveLoadGame::save_state), std::vector<std::uint8_t> (RE::BGSSaveLoadGame::*)() const>);
static_assert(std::same_as<decltype(&RE::BGSSaveLoadManager::save), std::vector<std::uint8_t> (RE::BGSSaveLoadManager::*)() const>);
static_assert(std::same_as<decltype(&RE::BeamProjectile::add_impact), RE::NamedUnsupportedHook (RE::BeamProjectile::*)() const>);
static_assert(std::same_as<decltype(RE::CALCED_OBJECT::form_id), std::uint32_t>);
static_assert(std::same_as<decltype(RE::CALCED_OBJECT::container_item), RE::ContainerItemExtra>);
static_assert(static_cast<std::uint32_t>(RE::CHARACTER_FLAGS::kNoCharacterCollisions) == (1u << 27));
struct Runtime final : RE::WorldBindings {
    std::vector<std::uint8_t> save_state() const override { return {1, 2, 3}; }
    bool load_state(const std::vector<std::uint8_t>& bytes, std::string&) override { return !bytes.empty(); }
    std::vector<RE::CALCED_OBJECT> calculate_current_form_list(std::uint32_t, std::uint16_t, std::int16_t, std::string&) override { return {{42, 3}}; }
    RE::NamedUnsupportedHook beam_projectile_impact() const override { return {"BeamProjectile::AddImpact"}; }
    RE::NamedUnsupportedHook character_flags() const override { return {"bhkCharacterController::flags"}; }
};
int main() {
    Runtime runtime;
    RE::BGSSaveLoadManager saves(runtime);
    RE::BeamProjectile beam(runtime);
    RE::CharacterControllerFlags flags(runtime);
    std::string error;
    const auto bytes = saves.save();
    if (!saves.load(bytes, error)) return 1;
    auto result = runtime.calculate_current_form_list(1, 1, 1, error);
    return bytes.size() == 3 && result.at(0).count == 3 &&
        std::string(saves.get_change().name) == "BGSSaveLoadGame::GetChange" &&
        std::string(saves.save_file().name) == "BGSSaveLoadManager::Save" &&
        std::string(beam.add_impact().name) == "BeamProjectile::AddImpact" &&
        std::string(flags.get().name) == "bhkCharacterController::flags" ? 0 : 1;
}
"#,
    )
    .unwrap();
    let binary = scratch.join("contract");
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++20 compiler is required by SHM-06");
    assert!(
        output.status.success(),
        "C++ contract compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(binary).status().expect("run C++ fixture");
    assert!(run.success(), "C++ fixture returned {run}");
    fs::remove_dir_all(scratch).unwrap();
}
