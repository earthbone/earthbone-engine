// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_05_cpp_contract_delegates_live_state_and_names_unsupported_hooks() {
    let root = crate_root();
    let scratch = std::env::temp_dir().join(format!("soe-shm-05-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contract.cpp");
    let binary = scratch.join("contract");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <array>
#include <cassert>
#include <concepts>
#include <optional>
#include <type_traits>

static_assert(std::same_as<decltype(&RE::WorldState::actor_value),
    std::optional<float> (RE::WorldState::*)(std::uint32_t, RE::ActorValue) const>);
static_assert(std::same_as<decltype(&RE::WorldState::set_actor_value),
    bool (RE::WorldState::*)(std::uint32_t, RE::ActorValue, float)>);
static_assert(std::same_as<decltype(&RE::WorldState::actor_value_name),
    std::optional<std::string> (RE::WorldState::*)(RE::ActorValue) const>);
static_assert(std::same_as<decltype(&RE::WorldState::actor_value_by_name),
    std::optional<RE::ActorValue> (RE::WorldState::*)(const std::string&) const>);
static_assert(std::same_as<decltype(&RE::WorldState::default_object),
    std::optional<RE::FormId> (RE::WorldState::*)(std::array<char, 4>) const>);

class FixtureState final : public RE::WorldState {
public:
    std::optional<float> actor_value(std::uint32_t actor, RE::ActorValue value) const override {
        return actor == 7 && value.value == RE::ActorValue::HEALTH.value ? std::optional<float>{health} : std::nullopt;
    }
    bool set_actor_value(std::uint32_t actor, RE::ActorValue value, float amount) override {
        if (actor != 7 || value.value != RE::ActorValue::HEALTH.value) return false;
        health = amount;
        return true;
    }
    std::optional<std::string> actor_value_name(RE::ActorValue value) const override {
        return value.value == RE::ActorValue::HEALTH.value ? std::optional<std::string>{"health"} : std::nullopt;
    }
    std::optional<RE::ActorValue> actor_value_by_name(const std::string& name) const override {
        return name == "health" ? std::optional<RE::ActorValue>{RE::ActorValue::HEALTH} : std::nullopt;
    }
    std::optional<RE::FormId> default_object(std::array<char, 4> key) const override {
        return key == std::array<char, 4>{'G', 'O', 'L', 'D'}
            ? std::optional<RE::FormId>{RE::FormId{0x00000014}}
            : std::nullopt;
    }
    float health{80.0F};
};

int main() {
    FixtureState state;
    RE::ActorValueList values{state};
    assert(values.get_actor_value_info(7, RE::ActorValue::HEALTH) == 80.0F);
    assert(values.set_actor_value(7, RE::ActorValue::HEALTH, 65.0F));
    assert(values.get_actor_value_info(7, RE::ActorValue::HEALTH) == 65.0F);
    assert(!values.set_actor_value(7, RE::ActorValue::MAGICKA, 10.0F));
    assert(values.get_actor_value_info(RE::ActorValue::HEALTH) == "health");
    assert(values.get_actor_value_name(RE::ActorValue::HEALTH) == "health");
    assert(values.lookup_actor_value_by_name("health")->value == RE::ActorValue::HEALTH.value);
    RE::BGSDefaultObjectManager defaults{state};
    assert(defaults.get({'G', 'O', 'L', 'D'})->value == 0x14);
    assert(!defaults.get({'N', 'O', 'N', 'E'}));
    assert(RE::ArrowProjectile::save_game() == RE::UnsupportedHook::ArrowProjectile);
    assert(RE::ArrowProjectile::load_game() == RE::UnsupportedHook::ArrowProjectile);
    assert(RE::ArrowProjectile::process_impacts() == RE::UnsupportedHook::ArrowProjectile);
    assert(RE::BGSLoadFormBuffer::get_version() == RE::UnsupportedHook::LoadFormBuffer);
    assert(RE::BGSLoadFormBuffer::load_form() == RE::UnsupportedHook::LoadFormBuffer);
}
"#,
    )
    .unwrap();

    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++20 compiler is required for the generated header contract");
    assert!(
        compile.status.success(),
        "C++ fixture did not compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(binary).output().unwrap();
    assert!(
        run.status.success(),
        "C++ fixture failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
}
