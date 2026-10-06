// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use soe_commonlib_shim::events::{ActorKill, CriticalHit, EventSink, NotifyControl};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fresh() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0xA00));
    let mut order = LoadOrder::new();
    order.add("Events.esm", plugin).unwrap();
    Game::new(order)
}

type Input = Rc<RefCell<BufWriter<std::process::ChildStdin>>>;

struct ActorKillPipe(Input);

impl EventSink<ActorKill> for ActorKillPipe {
    fn process(&mut self, event: &ActorKill) -> NotifyControl {
        let mut input = self.0.borrow_mut();
        writeln!(input, "K {} {}", event.killer.0, event.victim.0).unwrap();
        input.flush().unwrap();
        NotifyControl::Continue
    }
}

struct CriticalHitPipe(Input);

impl EventSink<CriticalHit> for CriticalHitPipe {
    fn process(&mut self, event: &CriticalHit) -> NotifyControl {
        let mut input = self.0.borrow_mut();
        writeln!(
            input,
            "C {} {} {}",
            event.aggressor.0,
            event.weapon.0,
            u8::from(event.sneak_hit)
        )
        .unwrap();
        input.flush().unwrap();
        NotifyControl::Continue
    }
}

fn cpp_sink_process() -> std::process::Child {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-18-bridge-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("bridge.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cstdint>
#include <iostream>
#include <memory>
#include <string>

template <class Event>
struct Capture final : RE::EventSink<Event> {
    int id;
    std::string& order;
    RE::NotifyControl result;
    Capture(int value, std::string& sequence, RE::NotifyControl control)
        : id(value), order(sequence), result(control) {}
    RE::NotifyControl process(const Event&) override {
        order += static_cast<char>('0' + id);
        return result;
    }
};

int main() {
    char kind;
    while (std::cin >> kind) {
        if (kind == 'K') {
            RE::ActorKill::Event event{};
            std::cin >> event.killer.value >> event.victim.value;
            RE::EventSource<RE::ActorKill::Event> source;
            std::string order;
            source.add_sink(std::make_unique<Capture<RE::ActorKill::Event>>(1, order, RE::NotifyControl::Continue));
            source.add_sink(std::make_unique<Capture<RE::ActorKill::Event>>(2, order, RE::NotifyControl::Continue));
            const auto seen = source.send(event);
            std::cout << "K " << event.killer.value << ' ' << event.victim.value << ' ' << seen << ' ' << order << '\n';
        } else if (kind == 'C') {
            RE::CriticalHit::Event event{};
            int sneakHit;
            std::cin >> event.aggressor.value >> event.weapon.value >> sneakHit;
            event.sneakHit = sneakHit != 0;
            RE::EventSource<RE::CriticalHit::Event> source;
            std::string order;
            source.add_sink(std::make_unique<Capture<RE::CriticalHit::Event>>(1, order, RE::NotifyControl::Stop));
            source.add_sink(std::make_unique<Capture<RE::CriticalHit::Event>>(2, order, RE::NotifyControl::Continue));
            const auto seen = source.send(event);
            std::cout << "C " << event.aggressor.value << ' ' << event.weapon.value << ' '
                      << event.sneakHit << ' ' << seen << ' ' << order << '\n';
        }
    }
    const auto hook = RE::install_hook(RE::UNSUPPORTED_VTABLE_HOOK);
    std::cout << "U " << hook.installed << ' ' << hook.unsupported.name << '\n';
    std::cout << "T " << SKSE::TRAMPOLINE.what << '\n';
}
"#,
    )
    .unwrap();
    let executable = scratch.join("bridge");
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(include)
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
    Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn shm_18_cpp_sinks_dispatch_live_world_kill_and_critical_hit_events() {
    let mut child = cpp_sink_process();
    let writer = Rc::new(RefCell::new(BufWriter::new(child.stdin.take().unwrap())));
    let game = fresh();
    game.world_mut()
        .actor_kill_events()
        .add_sink(Box::new(ActorKillPipe(writer.clone())));
    game.world_mut()
        .critical_hit_events()
        .add_sink(Box::new(CriticalHitPipe(writer.clone())));
    drop(writer);

    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    game.world_mut()
        .report_critical_hit(id(0x902), id(0xA00), true)
        .unwrap();
    assert!(game
        .world_mut()
        .report_critical_hit(id(0x902), id(0x901), false)
        .is_err());
    drop(game);

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let killer = (u64::from(id(0x901).file) << 32) | u64::from(id(0x901).object);
    let victim = (u64::from(id(0x900).file) << 32) | u64::from(id(0x900).object);
    let aggressor = (u64::from(id(0x902).file) << 32) | u64::from(id(0x902).object);
    let weapon = (u64::from(id(0xA00).file) << 32) | u64::from(id(0xA00).object);
    assert!(
        stdout.contains(&format!("K {killer} {victim} 2 12")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("C {aggressor} {weapon} 1 1 1")),
        "{stdout}"
    );
    assert!(stdout.contains("U 0 "), "{stdout}");
    assert!(
        stdout.contains("T SKSE::Trampoline patches machine code"),
        "{stdout}"
    );
}
