// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_10_cpp_extra_data_contracts_compile_and_reject_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-10-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("extras.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cstdint>
#include <type_traits>

static_assert(std::is_abstract_v<RE::ReferenceExtraQuery>);
static_assert(std::is_same_v<decltype(&RE::ReferenceExtraQuery::supports_extra), bool (RE::ReferenceExtraQuery::*)(RE::ReferenceExtraKind) const>);
static_assert(std::is_same_v<decltype(&RE::ReferenceExtraQuery::extra), std::optional<RE::ReferenceExtraValue> (RE::ReferenceExtraQuery::*)(RE::ObjectRefHandle, RE::ReferenceExtraKind) const>);
static_assert(std::is_same_v<decltype(&RE::HookRegistry::reject), bool (*)(const std::string&, std::string&)>);
static_assert(std::is_same_v<decltype(RE::ReferenceExtraValue::poisonCount), std::uint32_t>);

struct Fixture final : RE::ReferenceExtraQuery {
    bool supports_extra(RE::ReferenceExtraKind) const override { return true; }
    std::optional<RE::ReferenceExtraValue> extra(RE::ObjectRefHandle, RE::ReferenceExtraKind kind) const override {
        RE::ReferenceExtraValue value{};
        value.kind = kind;
        if (kind == RE::ReferenceExtraKind::MissingRefIDs)
            value.missingRefIDs.push_back({RE::FormId{0x01000001}, 0.25F});
        if (kind == RE::ReferenceExtraKind::NorthRotation) value.northRotation = 1.25F;
        if (kind == RE::ReferenceExtraKind::OutfitItem)
            value.outfitItem = RE::FormId{0x01000002};
        if (kind == RE::ReferenceExtraKind::Poison) { value.poison = RE::FormId{0x01000003}; value.poisonCount = UINT32_MAX; }
        if (kind == RE::ReferenceExtraKind::Teleport)
            value.teleport = RE::DoorTeleportData{RE::ObjectRefHandle{4}, {1, 2, 3}, {4, 5, 6}, 0};
        return value;
    }
};

int main() {
    Fixture fixture;
    const RE::ObjectRefHandle reference{7};
    for (auto kind : {RE::ReferenceExtraKind::MissingRefIDs, RE::ReferenceExtraKind::NorthRotation,
                      RE::ReferenceExtraKind::OutfitItem, RE::ReferenceExtraKind::Poison,
                      RE::ReferenceExtraKind::Teleport}) {
        if (!fixture.supports_extra(kind) || !fixture.extra(reference, kind)) return 1;
        const auto value = fixture.extra(reference, kind);
        if (kind == RE::ReferenceExtraKind::MissingRefIDs &&
            (value->missingRefIDs.size() != 1 || value->missingRefIDs[0].refID.value != 0x01000001 ||
             value->missingRefIDs[0].activateDelay != 0.25F)) return 5;
        if (kind == RE::ReferenceExtraKind::NorthRotation && value->northRotation != 1.25F) return 6;
        if (kind == RE::ReferenceExtraKind::OutfitItem &&
            (!value->outfitItem || value->outfitItem->value != 0x01000002)) return 7;
        if (kind == RE::ReferenceExtraKind::Poison && fixture.extra(reference, kind)->poisonCount != UINT32_MAX) return 4;
        if (kind == RE::ReferenceExtraKind::Poison &&
            (!value->poison || value->poison->value != 0x01000003)) return 8;
        if (kind == RE::ReferenceExtraKind::Teleport &&
            (!value->teleport || value->teleport->linkedDoor.value != 4 ||
             value->teleport->position.x != 1 || value->teleport->rotation.z != 6)) return 9;
    }
    std::string error;
    if (RE::HookRegistry::reject("write_call", error)) return 2;
    return error == "unsupported binary hook: write_call" ? 0 : 3;
}
"#,
    )
    .unwrap();

    let executable = scratch.join("extras");
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by SHM-10");
    assert!(
        output.status.success(),
        "C++ fixture compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("compiled C++ fixture should run");
    assert!(output.status.success(), "C++ fixture rejected its contract");
    fs::remove_dir_all(scratch).unwrap();
}
