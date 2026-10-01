// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_04_cpp_actor_contract_compiles() {
    let root = crate_root();
    let scratch = std::env::temp_dir().join(format!("soe-shm-04-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("actor_contract.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>

static_assert(std::same_as<RE::ActorPtr, struct RE::ActorPtr>);
static_assert(std::same_as<decltype(RE::ActorState{}.dead), bool>);
static_assert(std::same_as<decltype(RE::ActorState{}.unconscious), bool>);
static_assert(std::same_as<decltype(RE::ActorState{}.sneaking), bool>);
static_assert(std::same_as<decltype(RE::ActorState{}.weapon_drawn), bool>);
static_assert(std::same_as<decltype(RE::ActorState{}.blocking), bool>);
static_assert(std::same_as<decltype(&RE::Actor::actor_ptr), RE::ActorPtr (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::Actor::state), RE::ActorState (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::Actor::actor_value), std::optional<float> (RE::Actor::*)(RE::ActorValue) const>);
static_assert(std::same_as<decltype(&RE::Actor::set_actor_value), std::optional<RE::ActorApiError> (RE::Actor::*)(RE::ActorValue, float)>);
static_assert(std::same_as<decltype(&RE::ActorEquipManager::equip_object), std::optional<RE::ActorApiError> (RE::ActorEquipManager::*)(RE::ActorHandle, RE::FormId)>);
static_assert(std::same_as<decltype(&RE::ActorEquipManager::unequip_object), std::optional<RE::ActorApiError> (RE::ActorEquipManager::*)(RE::ActorHandle, RE::FormId)>);
static_assert(std::same_as<decltype(&RE::ActorEquipManager::reject_hook), std::optional<RE::ActorHookError> (RE::ActorEquipManager::*)(RE::ActorHookError) const>);
static_assert(RE::ActorHookError::AddressPatch != RE::ActorHookError::CallDetour);
static_assert(RE::ActorHookError::VtableOverride != RE::ActorHookError::Trampoline);

int main() {
    RE::ActorPtr empty{};
    RE::ActorState state{};
    return empty.handle.has_value() || state.dead ? 1 : 0;
}
",
    )
    .unwrap();

    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .output()
        .expect("C++20 compiler should be installed");
    assert!(
        output.status.success(),
        "SHM-04 C++ contract failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn shm_04_unsupported_hooks_are_explicit_errors() {
    use soe_commonlib_shim::form::FormId;
    use soe_commonlib_shim::refr::{ActorEquipManager, ActorHandle, HookError};

    struct EmptyManager;
    impl ActorEquipManager for EmptyManager {
        fn equip_object(
            &mut self,
            _: ActorHandle,
            _: FormId,
        ) -> Result<(), soe_commonlib_shim::refr::ActorApiError> {
            Err(soe_commonlib_shim::refr::ActorApiError::InvalidActor)
        }
        fn unequip_object(
            &mut self,
            _: ActorHandle,
            _: FormId,
        ) -> Result<(), soe_commonlib_shim::refr::ActorApiError> {
            Err(soe_commonlib_shim::refr::ActorApiError::InvalidActor)
        }
    }

    let manager = EmptyManager;
    for hook in [
        HookError::AddressPatch,
        HookError::CallDetour,
        HookError::VtableOverride,
        HookError::Trampoline,
    ] {
        assert_eq!(manager.reject_hook(hook), Err(hook));
    }
}
