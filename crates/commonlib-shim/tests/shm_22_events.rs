// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_22_event_headers_match_the_rust_payload_contract() {
    let events = fs::read_to_string(crate_root().join("include/RE/Events.h")).unwrap();
    for declaration in [
        "struct TESGrabReleaseEvent",
        "struct TESHarvestedEvent",
        "struct TESHitEvent",
        "struct TESObjectLoadedEvent",
        "struct TESQuestStageEvent",
        "enum class HookKind",
        "request_hook",
    ] {
        assert!(events.contains(declaration), "missing {declaration}");
    }
    assert!(matches!(
        soe_commonlib_shim::events::request_hook(soe_commonlib_shim::events::HookKind::Vtable),
        Err(soe_commonlib_shim::events::HookError::Unsupported(
            soe_commonlib_shim::events::HookKind::Vtable
        ))
    ));
}

#[test]
fn shm_22_cpp_event_fixture_calls_live_state_and_rejects_hooks() {
    let root = crate_root();
    let scratch = std::env::temp_dir().join(format!("soe-shm-22-events-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("events.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <cassert>
#include <map>

struct LiveState {
    std::map<std::uint32_t, bool> grabbed;
    std::map<std::uint32_t, std::uint32_t> inventory;
    std::map<std::uint32_t, float> health;
    std::map<std::uint32_t, bool> loaded;
    std::map<std::uint32_t, std::uint16_t> stages;

    void receive(const RE::TESGrabReleaseEvent& event) { grabbed[event.reference.value] = event.grabbed; }
    void receive(const RE::TESHarvestedEvent& event) { ++inventory[event.produceItem.value]; }
    void receive(const RE::TESHitEvent& event) {
        health[event.target.value] -= event.flags.contains(1) ? 5.0F : 2.0F;
    }
    void receive(const RE::TESObjectLoadedEvent& event) { loaded[event.formID.value] = event.loaded; }
    void receive(const RE::TESQuestStageEvent& event) { stages[event.formID.value] = event.stage; }
};

int main() {
    LiveState state;
    state.health[0x900] = 20.0F;
    state.receive(RE::TESGrabReleaseEvent{{0x901}, true});
    state.receive(RE::TESHarvestedEvent{{0x902}, {0x903}});
    state.receive(RE::TESHitEvent{{0x900}, {0x904}, {0xA00}, {0}, {1}});
    state.receive(RE::TESObjectLoadedEvent{{0x905}, true});
    state.receive(RE::TESQuestStageEvent{{0x906}, 40, 2});
    assert(state.grabbed[0x901]);
    assert(state.inventory[0x902] == 1);
    assert(state.health[0x900] == 15.0F);
    assert(state.loaded[0x905]);
    assert(state.stages[0x906] == 40);
    const auto call = RE::request_hook(RE::HookKind::CallSite);
    const auto branch = RE::request_hook(RE::HookKind::Branch);
    const auto vtable = RE::request_hook(RE::HookKind::Vtable);
    assert(call.code == RE::HookErrorCode::Unsupported && call.kind == RE::HookKind::CallSite);
    assert(branch.code == RE::HookErrorCode::Unsupported && branch.kind == RE::HookKind::Branch);
    assert(vtable.code == RE::HookErrorCode::Unsupported && vtable.kind == RE::HookKind::Vtable);
}
",
    )
    .unwrap();

    let executable = scratch.join("events");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by SHM-22");
    assert!(
        compile.status.success(),
        "C++ event fixture did not compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable).output().unwrap();
    assert!(
        run.status.success(),
        "C++ event fixture failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}
