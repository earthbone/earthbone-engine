// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_08_extra_data_contract_reads_live_values_and_rejects_mutation_hooks() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-08-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("extra_data.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>
#include <concepts>
#include <type_traits>
struct LiveStack final : RE::ExtraDataSource {
    bool has_type(RE::ExtraDataType kind) const override { return kind == RE::ExtraDataType::Enchantment || kind == RE::ExtraDataType::Hotkey || kind == RE::ExtraDataType::LeveledCreature; }
    std::optional<RE::ExtraEnchantment> enchantment() const override { return RE::ExtraEnchantment{{0x01001234}, 321, true}; }
    std::optional<RE::ExtraHotkey> hotkey() const override { return RE::ExtraHotkey{6}; }
    std::optional<RE::ExtraLeveledCreature> leveled_creature() const override { return RE::ExtraLeveledCreature{{0x02000011}, {0xFE001ABC}}; }
};
static_assert(static_cast<std::uint8_t>(RE::ExtraDataType::LeveledCreature) == 0x2D);
static_assert(static_cast<std::uint8_t>(RE::ExtraDataType::Hotkey) == 0x49);
static_assert(static_cast<std::uint8_t>(RE::ExtraDataType::Enchantment) == 0x9B);
static_assert(std::same_as<decltype(&RE::ExtraDataList::enchantment), std::optional<RE::ExtraEnchantment> (RE::ExtraDataList::*)() const>);
int main() {
    LiveStack live;
    RE::ExtraDataList list(live);
    assert(list.has_type(RE::ExtraDataType::Enchantment));
    assert((list.enchantment() == RE::ExtraEnchantment{{0x01001234}, 321, true}));
    assert((list.hotkey() == RE::ExtraHotkey{6}));
    assert((list.leveled_creature() == RE::ExtraLeveledCreature{{0x02000011}, {0xFE001ABC}}));
    assert(std::string(RE::ExtraDataList::ADD_UNSUPPORTED).find("mutates") != std::string::npos);
    assert(std::string(RE::ExtraDataList::REMOVE_UNSUPPORTED).find("mutates") != std::string::npos);
    assert(std::string(RE::ExtraDataList::REMOVE_BY_TYPE_UNSUPPORTED).find("mutates") != std::string::npos);
    assert(std::string(RE::ExtraDataList::SET_ENCHANTMENT_UNSUPPORTED).find("mutates") != std::string::npos);
}
"#,
    )
    .unwrap();
    let executable = scratch.join("extra_data");
    let compile = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C++20 compiler is required by SHM-08");
    assert!(
        compile.status.success(),
        "C++ contract did not compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(executable).status().unwrap();
    assert!(run.success());
    fs::remove_dir_all(scratch).unwrap();
}
