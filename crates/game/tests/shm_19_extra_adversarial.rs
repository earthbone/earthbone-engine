// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    cell::RefCell,
    fs,
    io::Write,
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    rc::Rc,
};

use soe_commonlib_shim::events::{
    DisarmedEvent, DragonSoulsGained, EventSink, LevelIncrease, LocationDiscovery, NotifyControl,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture(Rc<RefCell<Vec<LocationDiscovery>>>);

impl EventSink<LocationDiscovery> for Capture {
    fn process(&mut self, event: &LocationDiscovery) -> NotifyControl {
        self.0.borrow_mut().push(event.clone());
        NotifyControl::Continue
    }
}

struct CppConsumer(Rc<RefCell<ChildStdin>>);

impl CppConsumer {
    fn send(&self, kind: u8, fields: &[u8]) {
        let mut input = self.0.borrow_mut();
        input
            .write_all(&[kind])
            .expect("write event kind to C++ fixture");
        input
            .write_all(fields)
            .expect("write live event to C++ fixture");
        input.flush().expect("flush live event to C++ fixture");
    }
}

impl EventSink<DisarmedEvent> for CppConsumer {
    fn process(&mut self, event: &DisarmedEvent) -> NotifyControl {
        self.send(
            1,
            &[event.source.0.to_le_bytes(), event.target.0.to_le_bytes()].concat(),
        );
        NotifyControl::Continue
    }
}

impl EventSink<DragonSoulsGained> for CppConsumer {
    fn process(&mut self, event: &DragonSoulsGained) -> NotifyControl {
        self.send(2, &event.souls.to_le_bytes());
        NotifyControl::Continue
    }
}

impl EventSink<LevelIncrease> for CppConsumer {
    fn process(&mut self, event: &LevelIncrease) -> NotifyControl {
        let mut payload = event.player.0.to_le_bytes().to_vec();
        payload.extend_from_slice(&event.new_level.to_le_bytes());
        self.send(3, &payload);
        NotifyControl::Continue
    }
}

impl EventSink<LocationDiscovery> for CppConsumer {
    fn process(&mut self, event: &LocationDiscovery) -> NotifyControl {
        let mut payload = event.map_marker.0.to_le_bytes().to_vec();
        if let Some(worldspace) = &event.worldspace_id {
            payload.push(1);
            let length = u32::try_from(worldspace.len()).expect("worldspace id fits u32");
            payload.extend_from_slice(&length.to_le_bytes());
            payload.extend_from_slice(worldspace.as_bytes());
        } else {
            payload.push(0);
            payload.extend_from_slice(&0_u32.to_le_bytes());
        }
        self.send(4, &payload);
        NotifyControl::Continue
    }
}

fn cpp_consumer() -> (std::process::Child, Rc<RefCell<ChildStdin>>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-19-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("create C++ fixture directory");
    let source = scratch.join("live_events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <memory>
#include <string>

struct LiveState {
    unsigned count{};
    bool valid{true};
    void receive(const RE::DisarmedEvent& event) {
        valid = valid && event.source.value == 0x100000123ULL && event.target.value == 0x200000456ULL;
    }
    void receive(const RE::DragonSoulsGained& event) {
        valid = valid && std::abs(event.souls - 2.5F) < 0.001F;
    }
    void receive(const RE::LevelIncrease& event) {
        valid = valid && event.player.value == 0x14 && event.new_level == 17;
    }
    void receive(const RE::LocationDiscovery& event) {
        valid = valid && event.map_marker.value == 0x300000789ULL &&
            event.worldspace_id == std::optional<std::string>("17179871932");
    }
};

template <class Event>
struct Sink final : RE::EventSink<Event> {
    LiveState& state;
    explicit Sink(LiveState& value) : state(value) {}
    RE::NotifyControl process(const Event& event) override {
        state.receive(event);
        ++state.count;
        return RE::NotifyControl::Continue;
    }
};

template <class T>
bool read(T& value) { return std::fread(&value, sizeof(value), 1, stdin) == 1; }

int main() {
    RE::ScriptEventSourceHolder sources;
    LiveState state;
    sources.disarmed().add_sink(std::make_unique<Sink<RE::DisarmedEvent>>(state));
    sources.dragon_souls_gained().add_sink(std::make_unique<Sink<RE::DragonSoulsGained>>(state));
    sources.level_increase().add_sink(std::make_unique<Sink<RE::LevelIncrease>>(state));
    sources.location_discovery().add_sink(std::make_unique<Sink<RE::LocationDiscovery>>(state));

    std::uint8_t kind{};
    while (read(kind)) {
        if (kind == 1) {
            RE::DisarmedEvent event{};
            if (!read(event.source.value) || !read(event.target.value)) return 1;
            sources.disarmed().send(event);
        } else if (kind == 2) {
            RE::DragonSoulsGained event{};
            if (!read(event.souls)) return 2;
            sources.dragon_souls_gained().send(event);
        } else if (kind == 3) {
            RE::LevelIncrease event{};
            if (!read(event.player.value) || !read(event.new_level)) return 3;
            sources.level_increase().send(event);
        } else if (kind == 4) {
            RE::LocationDiscovery event{};
            std::uint8_t has_worldspace{};
            std::uint32_t length{};
            if (!read(event.map_marker.value) || !read(has_worldspace) || !read(length)) return 4;
            std::string worldspace(length, '\0');
            if (length && std::fread(worldspace.data(), 1, length, stdin) != length) return 5;
            if (has_worldspace) event.worldspace_id = std::move(worldspace);
            sources.location_discovery().send(event);
        } else {
            return 6;
        }
    }
    if (std::ferror(stdin) || state.count != 4 || !state.valid) return 7;
    const auto hook = RE::request_hook(RE::HookKind::Vtable);
    if (hook.code != RE::HookErrorCode::Unsupported || hook.kind != RE::HookKind::Vtable) return 8;
    return std::strcmp(SKSE::TRAMPOLINE.what,
        "SKSE::Trampoline patches machine code and has no target in this engine") == 0 ? 0 : 9;
}
"#,
    )
    .expect("write C++ live event fixture");
    let executable = scratch.join("live_events");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("../commonlib-shim/include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-19 done check");
    assert!(
        compile.status.success(),
        "live C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .spawn()
        .expect("start C++ live event consumer");
    let input = Rc::new(RefCell::new(
        child.stdin.take().expect("piped fixture stdin"),
    ));
    (child, input)
}

#[test]
fn discovery_is_once_even_when_fast_travel_is_enabled_later() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm19Map.esp", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(Capture(seen.clone())));

    game.world_mut().add_to_map(id(0x801), false);
    game.world_mut().add_to_map(id(0x801), true);

    assert_eq!(seen.borrow().len(), 1);
    assert!(game.world().reference_ref(id(0x801)).unwrap().fast_travel);
}

#[test]
fn shm_19_cpp_event_sinks_consume_live_world_events_and_reject_hooks() {
    let mut world = soe_game::World::new(LoadOrder::new());
    let (mut cpp, input) = cpp_consumer();
    world
        .commonlib_event_sources()
        .disarmed()
        .add_sink(Box::new(CppConsumer(input.clone())));
    world
        .commonlib_event_sources()
        .dragon_souls_gained()
        .add_sink(Box::new(CppConsumer(input.clone())));
    world
        .commonlib_event_sources()
        .level_increase()
        .add_sink(Box::new(CppConsumer(input.clone())));
    world
        .commonlib_event_sources()
        .location_discovery()
        .add_sink(Box::new(CppConsumer(input.clone())));

    world.commonlib_disarmed(
        GlobalFormId {
            file: 1,
            object: 0x123,
        },
        GlobalFormId {
            file: 2,
            object: 0x456,
        },
    );
    world.commonlib_dragon_souls_gained(2.5);
    world.player_level = 17;
    assert_eq!(world.commonlib_level_increase(), Ok(1));
    world.commonlib_location_discovery(
        GlobalFormId {
            file: 3,
            object: 0x789,
        },
        Some(GlobalFormId {
            file: 4,
            object: 0xabc,
        }),
    );
    drop(input);
    drop(world);

    let status = cpp.wait().expect("wait for C++ live event consumer");
    assert!(
        status.success(),
        "C++ live event consumer exited with {status}"
    );
}
