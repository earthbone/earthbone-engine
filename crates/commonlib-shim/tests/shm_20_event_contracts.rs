// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_20_typed_payloads_dispatch_and_unsupported_hooks_are_explicit() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-20-contract-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contracts.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Events.h>
#include <SKSE/Interfaces.h>
#include <cassert>
#include <functional>
#include <string>

template <class Event>
struct Sink final : RE::EventSink<Event> {
    std::function<void(const Event&)> check;
    explicit Sink(std::function<void(const Event&)> value) : check(std::move(value)) {}
    RE::NotifyControl process(const Event& event) override {
        check(event);
        return RE::NotifyControl::Continue;
    }
};

struct LiveState {
    RE::FormId shout{0x01000100};
    RE::ActorHandle player{7};
    RE::ActorHandle target{8};
    RE::FormId spell{0x01000200};
    RE::FormId cell{0x01000300};
    std::uint32_t skill{12};
    bool shout_attack(const RE::ShoutAttack& event) const { return event.shout.value == shout.value; }
    bool skill_increase(const RE::SkillIncrease& event) const {
        return event.player.value == player.value && event.actor_value.value == skill;
    }
    bool souls_trapped(const RE::SoulsTrapped& event) const {
        return event.trapper.value == player.value && event.target.value == target.value;
    }
    bool spell_learned(const RE::SpellsLearned& event) const { return event.spell.value == spell.value; }
    bool cell_loaded(const RE::TESCellFullyLoadedEvent& event) const { return event.cell.value == cell.value; }
};

int main() {
    LiveState state;
    RE::EventSource<RE::ShoutAttack> shout;
    shout.add_sink(std::make_unique<Sink<RE::ShoutAttack>>(
        [&](const RE::ShoutAttack& e) { assert(state.shout_attack(e)); }));
    assert(shout.send(RE::ShoutAttack{state.shout}) == 1);

    RE::EventSource<RE::SkillIncrease> skill;
    skill.add_sink(std::make_unique<Sink<RE::SkillIncrease>>(
        [&](const RE::SkillIncrease& e) { assert(state.skill_increase(e)); }));
    assert(skill.send(RE::SkillIncrease{state.player, RE::ActorValue{state.skill}}) == 1);

    RE::EventSource<RE::SoulsTrapped> souls;
    souls.add_sink(std::make_unique<Sink<RE::SoulsTrapped>>(
        [&](const RE::SoulsTrapped& e) { assert(state.souls_trapped(e)); }));
    assert(souls.send(RE::SoulsTrapped{state.player, state.target}) == 1);

    RE::EventSource<RE::SpellsLearned> spells;
    spells.add_sink(std::make_unique<Sink<RE::SpellsLearned>>(
        [&](const RE::SpellsLearned& e) { assert(state.spell_learned(e)); }));
    assert(spells.send(RE::SpellsLearned{state.spell}) == 1);

    RE::EventSource<RE::TESCellFullyLoadedEvent> cells;
    cells.add_sink(std::make_unique<Sink<RE::TESCellFullyLoadedEvent>>(
        [&](const RE::TESCellFullyLoadedEvent& e) { assert(state.cell_loaded(e)); }));
    assert(cells.send(RE::TESCellFullyLoadedEvent{state.cell}) == 1);
    assert(SKSE::TRAMPOLINE.what != nullptr);
    assert(std::string(SKSE::TRAMPOLINE.what).find("no target") != std::string::npos);
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("contracts"))
        .output()
        .expect("C++20 compiler is required by SHM-20");
    assert!(
        output.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(scratch.join("contracts")).status().unwrap();
    assert!(run.success());
    fs::remove_dir_all(scratch).unwrap();
}
