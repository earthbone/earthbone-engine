// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_07_cpp_contracts_call_calendar_character_defaults_and_extra_data() {
    let root = crate_root();
    let scratch = std::env::temp_dir().join(format!("soe-shm-07-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contracts.cpp");
    let binary = scratch.join("contracts");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <array>
#include <cassert>

struct Clock final : RE::Calendar {
    std::uint32_t year() const override { return 201; }
    std::uint32_t month() const override { return 7; }
    std::uint32_t day() const override { return 17; }
    double hour() const override { return 13.5; }
    double days_passed() const override { return 1.5; }
    double timescale() const override { return 20.0; }
};
struct Actor final : RE::Character {
    RE::FormId form_id() const override { return {0x14}; }
    std::optional<float> actor_value(const std::string& name) const override {
        return name == "health" ? std::optional<float>{100.0F} : std::nullopt;
    }
    bool is_dead() const override { return false; }
    std::optional<RE::FormId> race() const override { return RE::FormId{0x13746}; }
};
struct Defaults final : RE::DEFAULT_OBJECTS {
    Result get(const std::array<char, 4>& key) const override {
        return key == std::array<char, 4>{'D', 'F', 'T', 'S'}
            ? Result{true, RE::FormId{0x123}, {}} : Result{true, std::nullopt, {}};
    }
};
struct AliasArray final : RE::ExtraAliasInstanceArray {
    std::vector<RE::AliasInstance> instances(RE::FormId reference) const override {
        if (reference.value != 0x14) return {};
        return {{RE::FormId{0x200}, 4, RE::FormId{0x14}, {RE::FormId{0x300}}}};
    }
};
struct ActivateChildren final : RE::ExtraActivateRefChildren {
    ChildrenResult children(RE::FormId) const override {
        return {false, {}, RE::UnsupportedHook::ActivateReferenceChildren};
    }
    TimerResult timer(RE::FormId) const override {
        return {false, 0.0F, RE::UnsupportedHook::ActivateReferenceChildren};
    }
    std::optional<RE::UnsupportedHook> activate_children(RE::FormId) const override {
        return RE::UnsupportedHook::ActivateReferenceChildren;
    }
};
int main() {
    Clock clock; Actor actor; Defaults defaults; AliasArray aliases; ActivateChildren children;
    assert(clock.year() == 201 && clock.month() == 7 && clock.day() == 17);
    assert(clock.hour() == 13.5 && clock.days_passed() == 1.5 && clock.timescale() == 20.0);
    assert(actor.form_id().value == 0x14 && actor.actor_value("health") == 100.0F);
    assert(!actor.is_dead() && actor.race()->value == 0x13746);
    assert(defaults.get({'D', 'F', 'T', 'S'}).form->value == 0x123);
    const auto entries = aliases.instances({0x14});
    assert(entries.size() == 1 && entries[0].alias_id == 4);
    assert(entries[0].quest.value == 0x200 && entries[0].reference->value == 0x14);
    assert(entries[0].packages.size() == 1 && entries[0].packages[0].value == 0x300);
    assert(!children.children({0x14}).success);
    assert(children.children({0x14}).error == RE::UnsupportedHook::ActivateReferenceChildren);
    assert(!children.timer({0x14}).success);
    assert(children.timer({0x14}).error == RE::UnsupportedHook::ActivateReferenceChildren);
    assert(children.activate_children({0x14}) == RE::UnsupportedHook::ActivateReferenceChildren);
}
"#,
    )
    .unwrap();
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let compile_output = Command::new(compiler)
        .arg("-std=c++20")
        .arg("-Werror")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++ compiler is available");
    assert!(
        compile_output.status.success(),
        "C++ contract fixture failed to compile: {}",
        String::from_utf8_lossy(&compile_output.stderr)
    );
    let executed = Command::new(binary).output().expect("fixture runs");
    assert!(
        executed.status.success(),
        "C++ contract fixture failed: {}",
        String::from_utf8_lossy(&executed.stderr)
    );
}
