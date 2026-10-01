// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_16_cpp_contract_calls_world_views_and_reports_unsupported_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("soe-shm-16-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contract.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <cassert>
#include <type_traits>

static_assert(std::same_as<RE::TESDataHandler, RE::DataHandler>);
static_assert(std::same_as<RE::TESObjectREFR, RE::ObjectRef>);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kSheathed) == 0);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kWantToDraw) == 1);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kDrawing) == 2);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kDrawn) == 3);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kWantToSheathe) == 4);
static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kSheathing) == 5);
static_assert(std::same_as<decltype(&RE::TESObjectREFR::set_base),
                           RE::Unsupported (RE::TESObjectREFR::*)(RE::FormId)>);

struct Data final : RE::TESDataHandler {
    struct Entry final : RE::Form {
        RE::FormId form_id() const override { return {0x01000001}; }
        RE::FormType form_type() const override { return RE::FormType::new_type("MISC"); }
    } form;
    const RE::Form* lookup(RE::FormId id) const override {
        return id.value == 0x01000001 ? &form : nullptr;
    }
    const RE::Form* lookup_by_editor_id(const std::string&) const override { return &form; }
    std::vector<RE::FormId> forms_of_type(RE::FormType) const override { return {{0x01000001}}; }
    std::vector<std::string> files() const override { return {"Fixture.esm", "Light.esl"}; }
    std::optional<RE::FileInfo> file(std::size_t index) const override {
        if (index == 0) return RE::FileInfo{"Fixture.esm", 1, std::nullopt};
        if (index == 1) return RE::FileInfo{"Light.esl", std::nullopt, 0};
        return std::nullopt;
    }
};

struct Ref final : RE::TESObjectREFR {
    RE::FormId form_id() const override { return {0x01000002}; }
    RE::ObjectRefHandle handle() const override { return {0x01000002}; }
    RE::FormId base() const override { return {0x01000001}; }
    RE::NiPoint3 position() const override { return {1, 2, 3}; }
    RE::NiPoint3 rotation() const override { return {}; }
    std::optional<RE::FormId> parent_cell() const override { return std::nullopt; }
    bool is_disabled() const override { return false; }
    bool is_deleted() const override { return false; }
    RE::Unsupported set_base(RE::FormId) override { return {"TESObjectREFR::SetBaseObject"}; }
};

int main() {
    Data data;
    Ref ref;
    RE::TESObjectREFRPtr pointer{ref.handle()};
    assert(data.lookup(ref.base()) != nullptr);
    assert(data.files().at(0) == "Fixture.esm");
    assert(data.file(0)->regular_index == 1);
    assert(data.file(1)->is_light() && data.file(1)->light_index == 0);
    assert(pointer.handle->value == ref.form_id().value);
    assert(ref.position().z == 3);
    assert(std::string(ref.set_base({}).operation) == "TESObjectREFR::SetBaseObject");
    assert(RE::TESObjectREFRPtr{}.is_null());
    assert(std::string(SKSE::TRAMPOLINE.what).find("no target") != std::string::npos);
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("contract"))
        .output()
        .expect("C++20 compiler is required by SHM-16");
    assert!(
        output.status.success(),
        "C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(scratch.join("contract")).status().unwrap();
    assert!(run.success());
    fs::remove_dir_all(scratch).unwrap();
}
