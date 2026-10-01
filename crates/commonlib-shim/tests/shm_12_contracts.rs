// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn shm_12_cpp_contracts_compile_and_expose_explicit_unsupported_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-12-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("contracts.cpp");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>

static_assert(std::same_as<decltype(&RE::IFormFactory::GetFormType), RE::FormType (RE::IFormFactory::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::InventoryChanges::GetItemCount), std::int32_t (RE::InventoryChanges::*)(RE::FormId) const>);
static_assert(std::same_as<decltype(&RE::ReferenceResolver::lookup_reference_by_handle), bool (RE::ReferenceResolver::*)(RE::ObjectRefHandle, RE::LookupReferenceByHandleResult&) const>);

struct Resolver final : RE::ReferenceResolver {
    bool lookup_reference_by_handle(RE::ObjectRefHandle handle, RE::LookupReferenceByHandleResult& result) const override {
        result = {{handle.value}, RE::ReferenceKind::Actor};
        return handle.value != 0;
    }
};

int main() {
    RE::IFormFactory factory{"REFR", RE::FormType::new_type("REFR")};
    if (factory.GetFormType().as_str() != "REFR") return 1;
    if (factory.Create() != RE::UnsupportedHook::FormCreation) return 2;
    RE::InventoryChanges changes{{1}, {{{7}, 4, {}, false}, {{7}, 4, {22}, true}}, {{{7}, 5}}};
    if (changes.GetItemCount({7}) != 5) return 3;
    if (changes.AddEntryData({{8}, 1, {}, false}) != RE::UnsupportedHook::InventoryMutation) return 4;
    RE::ItemList items{changes.entries};
    if (items.GetSelectedItem() != RE::UnsupportedHook::ItemSelection) return 5;
    if (items.Update() != RE::UnsupportedHook::ItemListUpdate) return 6;
    Resolver resolver;
    RE::LookupReferenceByHandleResult result{};
    if (!RE::LookupReferenceByHandle(resolver, {9}, result)) return 7;
    return result.form_id.value == 9 && result.kind == RE::ReferenceKind::Actor ? 0 : 8;
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
        .arg(scratch.join("contracts"))
        .output()
        .expect("a C++20 compiler is required by the SHM-12 done check");
    assert!(
        output.status.success(),
        "C++ contract fixture failed to compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(scratch.join("contracts"))
        .output()
        .expect("compiled C++ contract fixture should run");
    assert!(
        run.status.success(),
        "C++ contract fixture returned failure"
    );
    fs::remove_dir_all(scratch).unwrap();
}
