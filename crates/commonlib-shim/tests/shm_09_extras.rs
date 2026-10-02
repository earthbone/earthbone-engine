// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_09_extra_data_headers_match_the_rust_api() {
    let include = crate_root().join("include");
    let reference = fs::read_to_string(include.join("RE/Reference.h")).unwrap();
    for declaration in [
        "struct ExtraLeveledItemBase",
        "struct ExtraLightData",
        "struct LinkedRefChild",
        "struct ExtraMapMarker",
        "struct MissingLinkedRefId",
        "class ReferenceExtraData",
        "EXTRA_DATA_HOOKS",
        "vtable hooks",
    ] {
        assert!(reference.contains(declaration), "missing {declaration}");
    }

    let scratch = std::env::temp_dir().join(format!("soe-shm-09-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("shm_09.cpp");
    let executable = scratch.join("shm_09");
    fs::write(
        &source,
        r#"#include <RE/Skyrim.h>
#include <cassert>

struct LiveExtras final : RE::ReferenceExtraData {
    std::optional<RE::ExtraLeveledItemBase> leveled_item_base() const override {
        return RE::ExtraLeveledItemBase{RE::FormId{0x123}};
    }
    std::optional<RE::ExtraLightData> light_data() const override {
        return RE::ExtraLightData{1.0F, 2.0F, 3.0F, 4.0F, 5};
    }
    const std::vector<RE::LinkedRefChild>& linked_ref_children() const override { return children; }
    std::optional<RE::ExtraMapMarker> map_marker() const override {
        return RE::ExtraMapMarker{"Whiterun", 3, 1};
    }
    const std::vector<RE::MissingLinkedRefId>& missing_linked_ref_ids() const override { return missing; }
    std::vector<RE::LinkedRefChild> children{{RE::FormId{0x456}, RE::ObjectRefHandle{9}}};
    std::vector<RE::MissingLinkedRefId> missing{{RE::FormId{0x456}, RE::FormId{0x789}}};
};

int main() {
    LiveExtras live;
    assert(live.leveled_item_base()->leveled_item.value == 0x123);
    assert(live.light_data()->shadow_depth_bias == 4.0F);
    assert(live.linked_ref_children().at(0).reference.value == 9);
    assert(live.map_marker()->name == "Whiterun");
    assert(live.missing_linked_ref_ids().at(0).linked_reference.value == 0x789);
    assert(RE::EXTRA_DATA_HOOKS.reason != nullptr);
}
"#,
    )
    .unwrap();
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("a C++20 compiler is required by the SHM-09 done check");
    assert!(
        output.status.success(),
        "C++ fixture failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("the C++ fixture executable should run");
    assert!(
        output.status.success(),
        "C++ fixture assertions failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn shm_09_extra_data_hooks_report_the_unsupported_boundary() {
    let hook = soe_commonlib_shim::refr::EXTRA_DATA_HOOKS
        .first()
        .expect("the unsupported extra-data hook is declared");
    assert_eq!(hook.name, "CommonLib extra-data vtable hooks");
    assert_eq!(
        hook.reason,
        "Skyrim executable vtables have no target in the Rust engine"
    );
}
