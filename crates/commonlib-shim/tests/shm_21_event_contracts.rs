// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
#![allow(clippy::needless_raw_string_hashes)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_21_event_contracts_call_live_state_and_reject_hooks() {
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

struct State final : RE::EventState {
    std::optional<std::int32_t> inventory_count(RE::FormId container, RE::FormId item) const override {
        if (container.value == 10 && item.value == 20) return 3;
        return std::nullopt;
    }
    std::optional<bool> is_dead(RE::ActorHandle actor) const override {
        return actor.value == 30 ? std::optional<bool>{true} : std::nullopt;
    }
    std::optional<bool> is_in_bleedout(RE::ActorHandle actor) const override {
        return actor.value == 31 ? std::optional<bool>{true} : std::nullopt;
    }
    std::optional<bool> is_form_deleted(RE::FormId form) const override {
        return form.value == 40 ? std::optional<bool>{true} : std::nullopt;
    }
    std::optional<RE::TESFurnitureEvent> furniture_event(RE::ActorHandle actor) const override {
        if (actor.value != 50) return std::nullopt;
        return RE::TESFurnitureEvent{{50}, {51}, RE::FurnitureEventType::Enter};
    }
};

int main() {
    State state;
    RE::TESContainerChangedEvent moved{{}, RE::FormId{10}, RE::FormId{20}, 3, {60}, 7};
    RE::TESDeathEvent death{{30}, RE::ObjectRefHandle{32}, true};
    RE::TESEnterBleedoutEvent bleedout{{31}};
    RE::TESFormDeleteEvent deleted{{40}};
    RE::TESFurnitureEvent furniture{{50}, {51}, RE::FurnitureEventType::Enter};
    assert(state.inventory_count(*moved.newContainer, moved.baseObj) == moved.itemCount);
    assert(state.is_dead({death.actorDying.value}) == death.dead);
    assert(state.is_in_bleedout({bleedout.actor.value}).value());
    assert(state.is_form_deleted(deleted.formID).value());
    assert(state.furniture_event({furniture.actor.value})->targetFurniture.value == 51);
    assert(RE::UNSUPPORTED_VTABLE_HOOK.reason[0] != '\0');
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
