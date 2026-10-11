// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![allow(clippy::needless_raw_string_hashes)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_21_event_contracts_expose_payloads_and_reject_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-21-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("event_contracts.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>
#include <type_traits>

int main() {
    RE::TESContainerChangedEvent moved{{}, RE::FormId{10}, RE::FormId{20}, 3, {60}, 7};
    RE::TESDeathEvent death{{30}, RE::ObjectRefHandle{32}, true};
    RE::TESEnterBleedoutEvent bleedout{{31}};
    RE::TESFormDeleteEvent deleted{{40}};
    RE::TESFurnitureEvent furniture{{50}, {51}, RE::FurnitureEventType::Enter};
    static_assert(std::is_same_v<decltype(moved.itemCount), std::int32_t>);
    static_assert(std::is_same_v<decltype(death.dead), bool>);
    static_assert(std::is_same_v<decltype(bleedout.actor), RE::ObjectRefHandle>);
    static_assert(std::is_same_v<decltype(deleted.formID), RE::FormId>);
    static_assert(std::is_same_v<decltype(furniture.type), RE::FurnitureEventType>);
    assert(moved.baseObj.value == 20 && moved.itemCount == 3);
    assert(death.actorDying.value == 30 && death.actorKiller->value == 32 && death.dead);
    assert(bleedout.actor.value == 31 && deleted.formID.value == 40);
    assert(furniture.actor.value == 50 && furniture.targetFurniture.value == 51);
    assert(furniture.type == RE::FurnitureEventType::Enter);
    const auto hook = RE::request_hook(RE::HookKind::Vtable);
    assert(hook.code == RE::HookErrorCode::Unsupported);
    assert(hook.kind == RE::HookKind::Vtable);
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("event_contracts"))
        .output()
        .expect("C++20 compiler is required by SHM-21");
    assert!(
        output.status.success(),
        "C++ contract compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(scratch.join("event_contracts"))
        .status()
        .unwrap()
        .success());
    fs::remove_dir_all(scratch).unwrap();
}
