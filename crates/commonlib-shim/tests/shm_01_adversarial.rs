// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_01_event_source_delivers_in_order_and_stops_after_stop() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch =
        std::env::temp_dir().join(format!("soe-shm-01-adversarial-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("events.cpp");
    fs::write(
        &source,
        r"
#include <RE/Events.h>
#include <cassert>
struct Sink final : RE::EventSink<RE::ScriptEvent> {
    int* calls;
    RE::NotifyControl result;
    Sink(int* count, RE::NotifyControl value) : calls(count), result(value) {}
    RE::NotifyControl process(const RE::ScriptEvent&) override { ++*calls; return result; }
};
int main() {
    int calls = 0;
    RE::EventSource<RE::ScriptEvent> source;
    source.add_sink(std::make_unique<Sink>(&calls, RE::NotifyControl::Continue));
    source.add_sink(std::make_unique<Sink>(&calls, RE::NotifyControl::Stop));
    source.add_sink(std::make_unique<Sink>(&calls, RE::NotifyControl::Continue));
    assert(source.send(RE::ScriptEvent::ContainerChanged) == 2);
    assert(calls == 2);
}
",
    )
    .unwrap();
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("events"))
        .output()
        .expect("C++20 compiler is required by SHM-01");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(scratch.join("events")).status().unwrap();
    assert!(run.success());
    fs::remove_dir_all(scratch).unwrap();
}
