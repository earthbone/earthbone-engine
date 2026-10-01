// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_23_cpp_contract_fixture_matches_payload_and_rejects_hooks() {
    let root = crate_root();
    let scratch = std::env::temp_dir().join(format!("soe-shm-23-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create fixture directory");
    let source = scratch.join("quest_start_stop.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cstddef>
#include <cstring>
#include <type_traits>

static_assert(sizeof(RE::TESQuestStartStopEvent) == 0x8);
static_assert(offsetof(RE::TESQuestStartStopEvent, formID) == 0x0);
static_assert(offsetof(RE::TESQuestStartStopEvent, started) == 0x4);
static_assert(offsetof(RE::TESQuestStartStopEvent, failed) == 0x5);
static_assert(offsetof(RE::TESQuestStartStopEvent, pad06) == 0x6);
static_assert(std::is_same_v<decltype(SKSE::TRAMPOLINE), const SKSE::Unsupported>);

struct QuestSink final : RE::EventSink<RE::TESQuestStartStopEvent> {
    bool saw{};
    RE::NotifyControl process(const RE::TESQuestStartStopEvent& event) override {
        saw = event.formID == 0x01000800 && !event.started && event.failed && event.pad06 == 0;
        return RE::NotifyControl::Continue;
    }
};

int main() {
    RE::EventSource<RE::TESQuestStartStopEvent> source;
    auto sink = std::make_unique<QuestSink>();
    auto* observed = sink.get();
    source.add_sink(std::move(sink));
    const RE::TESQuestStartStopEvent event{0x01000800, false, true, 0};
    if (source.send(event) != 1 || !observed->saw) return 1;
    return std::strcmp(SKSE::TRAMPOLINE.what,
        "SKSE::Trampoline patches machine code and has no target in this engine") == 0 ? 0 : 2;
}
"#,
    )
    .expect("write C++ fixture");

    let executable = scratch.join("quest_start_stop");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-23 done check");
    assert!(
        compile.status.success(),
        "C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .status()
        .expect("run C++ contract fixture");
    assert!(run.success(), "C++ contract fixture rejected the API");
    fs::remove_dir_all(scratch).expect("remove fixture directory");
}
