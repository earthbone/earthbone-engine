// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_18_cpp_event_types_dispatch_and_unsupported_hooks_are_explicit() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-18-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cassert>
#include <string_view>
#include <type_traits>

static_assert(static_cast<int>(RE::NotifyControl::Continue) == 0);
static_assert(static_cast<int>(RE::NotifyControl::Stop) == 1);
static_assert(sizeof(RE::ActorKill::Event) == 0x10);
static_assert(sizeof(RE::CriticalHit::Event) == 0x18);
static_assert(std::is_same_v<decltype(RE::ActorKill::Event::killer), RE::EventFormId>);
static_assert(std::is_same_v<decltype(RE::ActorKill::Event::victim), RE::EventFormId>);
static_assert(std::is_same_v<decltype(RE::CriticalHit::Event::aggressor), RE::EventFormId>);
static_assert(std::is_same_v<decltype(RE::CriticalHit::Event::weapon), RE::EventFormId>);
static_assert(std::is_same_v<decltype(RE::CriticalHit::Event::sneakHit), bool>);

template <class Event>
struct Counter final : RE::EventSink<Event> {
    int* calls;
    RE::NotifyControl result;
    Counter(int* count, RE::NotifyControl value) : calls(count), result(value) {}
    RE::NotifyControl process(const Event&) override { ++*calls; return result; }
};

int main() {
    const RE::ActorKill::Event killed{{0x900}, {0x901}};
    RE::EventSource<RE::ActorKill::Event> kills;
    int killCalls = 0;
    kills.add_sink(std::make_unique<Counter<RE::ActorKill::Event>>(&killCalls, RE::NotifyControl::Continue));
    assert(kills.send(killed) == 1 && killCalls == 1);

    const RE::CriticalHit::Event critical{{0x900}, {0xA00}, true};
    RE::EventSource<RE::CriticalHit::Event> criticalHits;
    int criticalCalls = 0;
    criticalHits.add_sink(std::make_unique<Counter<RE::CriticalHit::Event>>(&criticalCalls, RE::NotifyControl::Stop));
    int skippedCalls = 0;
    criticalHits.add_sink(std::make_unique<Counter<RE::CriticalHit::Event>>(&skippedCalls, RE::NotifyControl::Continue));
    assert(criticalHits.send(critical) == 1 && criticalCalls == 1 && skippedCalls == 0);
    assert(std::string_view(SKSE::TRAMPOLINE.what).find("no target") != std::string_view::npos);
}
"#,
    )
    .unwrap();
    let executable = scratch.join("events");
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C++20 compiler is required by SHM-18");
    assert!(
        output.status.success(),
        "C++ compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(executable).status().unwrap().success());
    fs::remove_dir_all(scratch).unwrap();
}
