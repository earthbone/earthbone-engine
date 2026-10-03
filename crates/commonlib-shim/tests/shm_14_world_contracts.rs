// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_14_cpp_contracts_compile_and_reject_unsupported_projectile_runtime() {
    let root = crate_root();
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-14-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contracts.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>
static_assert(sizeof(RE::ObjectRefHandle) == sizeof(std::uint32_t));
static_assert(sizeof(RE::ProjectileHandle) == sizeof(std::uint32_t));
static_assert(std::same_as<decltype(&RE::PlayerCharacter::form_key), std::optional<RE::FormKey> (RE::PlayerCharacter::*)()>);
static_assert(std::same_as<decltype(&RE::PlayerCharacter::handle), std::optional<RE::ObjectRefHandle> (RE::PlayerCharacter::*)()>);
static_assert(std::same_as<decltype(&RE::PlayerCharacter::position), std::optional<RE::Position> (RE::PlayerCharacter::*)()>);
static_assert(std::same_as<decltype(&RE::PlayerCharacter::actor_value), std::optional<float> (RE::PlayerCharacter::*)(const char*)>);
static_assert(std::same_as<decltype(&RE::ProcessLists::actor_forms), std::vector<RE::FormKey> (RE::ProcessLists::*)() const>);
static_assert(std::same_as<decltype(&RE::Projectile::runtime), RE::UnsupportedHook (RE::Projectile::*)() const noexcept>);
int main() {
    RE::Projectile projectile(RE::ProjectileHandle{7});
    return projectile.handle().value == 7 &&
        projectile.runtime() == RE::UnsupportedHook::ProjectileRuntime ? 0 : 1;
}
",
    )
    .unwrap();
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("contracts"))
        .output()
        .expect("a C++20 compiler is required by SHM-14");
    assert!(
        output.status.success(),
        "C++ contract compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(scratch.join("contracts"))
        .status()
        .expect("the compiled C++ contract fixture should run");
    assert!(
        run.success(),
        "C++ world contract fixture rejected its result"
    );
    fs::remove_dir_all(scratch).unwrap();
}
