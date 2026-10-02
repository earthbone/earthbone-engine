// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_19_cpp_contract_fixture_covers_typed_sources_and_hook_rejection() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-19-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("events.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>
static_assert(std::same_as<decltype(&RE::ScriptEventSourceHolder::disarmed), RE::EventSource<RE::DisarmedEvent>& (RE::ScriptEventSourceHolder::*)() noexcept>);
static_assert(std::same_as<decltype(&RE::ScriptEventSourceHolder::dragon_souls_gained), RE::EventSource<RE::DragonSoulsGained>& (RE::ScriptEventSourceHolder::*)() noexcept>);
static_assert(std::same_as<decltype(&RE::ScriptEventSourceHolder::level_increase), RE::EventSource<RE::LevelIncrease>& (RE::ScriptEventSourceHolder::*)() noexcept>);
static_assert(std::same_as<decltype(&RE::ScriptEventSourceHolder::location_discovery), RE::EventSource<RE::LocationDiscovery>& (RE::ScriptEventSourceHolder::*)() noexcept>);
struct DisarmedSink final : RE::EventSink<RE::DisarmedEvent> { RE::NotifyControl process(const RE::DisarmedEvent&) override { return RE::NotifyControl::Continue; } };
int main() {
  RE::ScriptEventSourceHolder sources;
  sources.disarmed().add_sink(std::make_unique<DisarmedSink>());
  const RE::DisarmedEvent disarmed{{1}, {2}};
  const RE::DragonSoulsGained souls{1.0F};
  const RE::LevelIncrease level{{3}, 2};
  const RE::LocationDiscovery location{{4}, std::string("Skyrim")};
  sources.disarmed().send(disarmed);
  sources.dragon_souls_gained().send(souls);
  sources.level_increase().send(level);
  sources.location_discovery().send(location);
  const auto rejected = RE::install_hook({"vtable"});
  return !rejected.installed && rejected.unsupported.name[0] == 'v' ? 0 : 1;
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(include)
        .arg(source)
        .output()
        .expect("C++20 compiler is required for the SHM-19 contract fixture");
    assert!(
        output.status.success(),
        "C++ contract fixture failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn shm_19_unsupported_hook_is_rejected_with_its_name() {
    let hook = soe_commonlib_shim::events::UnsupportedHook::new("write_vfunc");
    assert_eq!(soe_commonlib_shim::events::install_hook(hook), Err(hook));
}
