// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_13_generated_magic_header_matches_the_mit_contract() {
    let root = crate_root();
    let generated = soe_commonlib_shim::headers::generated_headers();
    let expected = generated
        .iter()
        .find(|(path, _)| *path == "RE/Magic.h")
        .expect("SHM-13 Magic header is generated");
    assert_eq!(
        fs::read_to_string(root.join("include/RE/Magic.h")).unwrap(),
        expected.1
    );
    let umbrella = fs::read_to_string(root.join("include/RE/Skyrim.h")).unwrap();
    assert!(umbrella.contains("#include \"RE/Magic.h\""));
}

#[test]
fn shm_13_cpp_contract_calls_magic_world_interfaces_and_reports_unsupported_hooks() {
    let root = crate_root();
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-13-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("magic.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <cassert>
#include <set>
struct Favorites final : RE::MagicFavorites {
    std::set<std::uint32_t> forms;
    bool is_favorite(RE::FormId form) const override { return forms.contains(form.value); }
    bool set_favorite(RE::FormId form, bool favorite) override {
        if (favorite) return forms.insert(form.value).second;
        return forms.erase(form.value) != 0;
    }
};
struct System final : RE::MagicSystem {
    RE::CannotCastReason reason{RE::CannotCastReason::Ok};
    RE::CannotCastReason cannot_cast(RE::FormId, RE::FormId) const override { return reason; }
};
struct Target final : RE::MagicTarget {
    std::set<std::uint32_t> effects;
    std::set<std::uint32_t> spells;
    bool has_magic_effect(RE::FormId effect) const override { return effects.contains(effect.value); }
    std::size_t active_effect_count() const override { return effects.size(); }
    bool dispel_spell(RE::FormId spell) override { return spells.erase(spell.value) != 0; }
};
int main() {
    Favorites favorites;
    assert(favorites.set_favorite({42}, true));
    assert(favorites.is_favorite({42}));
    assert(favorites.set_favorite({42}, false));
    assert(!favorites.is_favorite({42}));

    System system;
    assert(system.cannot_cast({1}, {2}) == RE::CannotCastReason::Ok);
    system.reason = RE::CannotCastReason::Magicka;
    assert(system.cannot_cast({1}, {2}) == RE::CannotCastReason::Magicka);

    Target target;
    target.effects.insert(7);
    target.spells.insert(9);
    assert(target.has_magic_effect({7}) && target.active_effect_count() == 1);
    assert(target.dispel_spell({9}) && !target.dispel_spell({9}));

    RE::MapMarkerData marker;
    marker.set_visible(true);
    marker.set_can_travel_to(true);
    assert(marker.visible() && marker.can_travel_to());
    marker.set_visible(false);
    assert(!marker.visible() && marker.can_travel_to());

    RE::ObjectEquipParams params;
    assert(!params.unsupported());
    params.has_equip_slot = true;
    assert(params.unsupported()->hook == RE::UnsupportedHook::UnsupportedEquipOption);
    assert(RE::Unsupported{RE::UnsupportedHook::BinaryAddress}.hook == RE::UnsupportedHook::BinaryAddress);
    assert(RE::Unsupported{RE::UnsupportedHook::VtablePatch}.hook == RE::UnsupportedHook::VtablePatch);
}
",
    )
    .unwrap();
    let binary = scratch.join("magic");
    let output = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(&include)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by the SHM-13 contract check");
    assert!(
        output.status.success(),
        "C++ contract compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(binary).output().unwrap();
    assert!(
        output.status.success(),
        "C++ contract fixture failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}
