// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_11_generated_headers_include_the_world_type_contract() {
    let include = crate_root().join("include");
    let generated = soe_commonlib_shim::headers::generated_headers();
    assert_eq!(generated.len(), 13);
    for (relative, expected) in generated {
        let path = include.join(relative);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            *expected,
            "{} is stale",
            path.display()
        );
    }
    let umbrella = fs::read_to_string(include.join("RE/Skyrim.h")).unwrap();
    assert!(umbrella.contains("RE/WorldTypes.h"));
}

#[test]
fn shm_11_cpp_contract_fixtures_call_live_state_and_report_unsupported_hooks() {
    let include = crate_root().join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-11-contract-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("world_types.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>

struct Fixture final : RE::WorldTypes, RE::GameSettingCollection {
    std::optional<RE::SettingValue> get_setting(const std::string& name) const override {
        if (name != "fCombatDistance") return std::nullopt;
        return RE::SettingValue{RE::SettingValue::Kind::Float, false, 0, 96.0F, {}};
    }
    std::optional<RE::WornState> extra_worn(RE::FormId, RE::FormId item) const override {
        return item.value == 0x01000002 ? std::optional<RE::WornState>{{true, false}} : std::nullopt;
    }
    std::optional<RE::HazardState> hazard(RE::FormId ref) const override {
        if (ref.value != 0x01000003) return std::nullopt;
        return RE::HazardState{ref, {0x01000004}, {1.0F, 2.0F, 3.0F}, true};
    }
    std::optional<RE::HighProcessState> high_process(RE::FormId actor) const override {
        return RE::HighProcessState{actor, true, RE::FormId{0x01000005}, true};
    }
    std::optional<RE::HitData> resolve_hit(RE::FormId attacker, RE::FormId target, RE::FormId weapon) override {
        return RE::HitData{attacker, target, weapon, 12.0F, 0.2F, 9.6F, false, false};
    }
    RE::UnsupportedHookInfo unsupported_hook(const std::string& type, const std::string& hook) const override {
        if (type == "Hazard" && hook == "tick") return {type, hook, "hazard simulation is not implemented"};
        return {type, hook, "hook is outside the implemented source-level boundary"};
    }
};

int main() {
    Fixture state;
    const auto worn = state.extra_worn({0x01000001}, {0x01000002});
    const auto setting = state.get_setting("fCombatDistance");
    const auto hazard = state.hazard({0x01000003});
    const auto process = state.high_process({0x01000001});
    const auto hit = state.resolve_hit({0x01000001}, {0x01000005}, {0x01000002});
    const auto unsupported = state.unsupported_hook("Hazard", "tick");
    assert(worn && worn->worn && !worn->worn_left);
    assert(setting && setting->kind == RE::SettingValue::Kind::Float && setting->float_value == 96.0F);
    assert(hazard && hazard->enabled && hazard->position.z == 3.0F);
    assert(process && process->in_combat && process->staggered && process->combat_target->value == 0x01000005);
    assert(hit && hit->health_damage == 9.6F && !hit->killed);
    assert(unsupported.reason == "hazard simulation is not implemented");
}
"#,
    )
    .unwrap();

    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("world_types"))
        .output()
        .expect("C++20 compiler is required by SHM-11");
    assert!(
        output.status.success(),
        "C++ compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(scratch.join("world_types")).output().unwrap();
    assert!(
        output.status.success(),
        "C++ fixture failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}
