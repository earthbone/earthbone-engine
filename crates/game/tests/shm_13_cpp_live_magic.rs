// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{MagicWorldState, ObjectEquipParams, UnsupportedState};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{commonlib_magic::CommonLibMagic, World};

fn fixture_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x10));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 1));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 2).sub(Tag(*b"SPIT"), vec![0; 36]));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 3));
    plugin.push_top(Record::new(Tag(*b"MISC"), 4));
    let mut actor = Record::new(Tag(*b"ACHR"), 0x20).sub(Tag(*b"NAME"), 1_u32.to_le_bytes());
    actor.placement.cell = Some(0x10);
    plugin.push_top(actor);
    let mut order = LoadOrder::new();
    order.add("shm_13_cpp_fixture.esp", plugin).unwrap();
    World::new(order)
}

fn global(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn runtime(world: &World, object: u32) -> u32 {
    world.order().runtime_id(global(object))
}

fn allocate(_name: &str, _class: &str) -> soe_papyrus::ObjectId {
    soe_papyrus::ObjectId(1)
}

#[test]
fn shm_13_cpp_magic_interfaces_dispatch_to_live_world_and_reject_unsupported_calls() {
    let shim = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim");
    let scratch = std::env::temp_dir().join(format!("soe-shm-13-cpp-live-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("magic.cpp");
    std::fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>
#include <iostream>
#include <string>
std::string request(const std::string& value) {
    std::cout << value << std::endl;
    std::string response;
    if (!std::getline(std::cin, response)) return "bridge-error";
    return response;
}
struct Favorites final : RE::MagicFavorites {
    RE::FormId actor;
    explicit Favorites(RE::FormId value) : actor(value) {}
    bool is_favorite(RE::FormId form) const override {
        return request("favorite_get " + std::to_string(actor.value) + " " + std::to_string(form.value)) == "true";
    }
    bool set_favorite(RE::FormId form, bool favorite) override {
        return request("favorite_set " + std::to_string(actor.value) + " " + std::to_string(form.value) + " " + (favorite ? "true" : "false")) == "changed";
    }
};
struct System final : RE::MagicSystem {
    RE::CannotCastReason cannot_cast(RE::FormId actor, RE::FormId spell) const override {
        return static_cast<RE::CannotCastReason>(std::stoi(request("cannot_cast " + std::to_string(actor.value) + " " + std::to_string(spell.value))));
    }
};
struct Target final : RE::MagicTarget {
    RE::FormId target;
    explicit Target(RE::FormId value) : target(value) {}
    bool has_magic_effect(RE::FormId effect) const override {
        return request("has_effect " + std::to_string(target.value) + " " + std::to_string(effect.value)) == "true";
    }
    std::size_t active_effect_count() const override {
        return static_cast<std::size_t>(std::stoul(request("effect_count " + std::to_string(target.value))));
    }
    bool dispel_spell(RE::FormId spell) override {
        return request("dispel " + std::to_string(target.value) + " " + std::to_string(spell.value)) == "true";
    }
};
struct Marker {
    RE::FormId reference;
    explicit Marker(RE::FormId value) : reference(value) {}
    bool refresh(RE::MapMarkerData& marker) const {
        const auto value = request("marker_get " + std::to_string(reference.value));
        if (value.starts_with("error")) return false;
        marker.flags = static_cast<std::uint8_t>(std::stoi(value));
        return true;
    }
    std::string write(const RE::MapMarkerData& marker) const {
        return request("marker_set " + std::to_string(reference.value) + " " + std::to_string(marker.flags));
    }
};
std::string equip(RE::FormId actor, RE::FormId item, const RE::ObjectEquipParams& params) {
    const auto unsupported = params.unsupported();
    const auto kind = unsupported ? "unsupported" : "supported";
    return request("equip " + std::to_string(actor.value) + " " + std::to_string(item.value) + " " + kind);
}
int main(int argc, char** argv) {
    const auto actor = RE::FormId{static_cast<std::uint32_t>(std::stoul(argv[1]))};
    const auto spell = RE::FormId{static_cast<std::uint32_t>(std::stoul(argv[2]))};
    const auto effect = RE::FormId{static_cast<std::uint32_t>(std::stoul(argv[3]))};
    const auto item = RE::FormId{static_cast<std::uint32_t>(std::stoul(argv[4]))};
    Favorites favorites(actor);
    if (favorites.is_favorite(spell)) return 1;
    if (!favorites.set_favorite(spell, true) || !favorites.is_favorite(spell)) return 2;
    System system;
    if (system.cannot_cast(actor, spell) != RE::CannotCastReason::Ok) return 3;
    Target target(actor);
    if (target.has_magic_effect(effect) || target.active_effect_count() != 0) return 4;
    if (target.dispel_spell(spell) || target.has_magic_effect(effect)) return 5;
    Marker marker(actor);
    RE::MapMarkerData marker_data;
    if (!marker.refresh(marker_data) || marker_data.visible()) return 6;
    marker_data.set_visible(true);
    marker_data.set_can_travel_to(true);
    if (marker.write(marker_data) != "ok" || !marker.refresh(marker_data) || !marker_data.visible() || !marker_data.can_travel_to()) return 7;
    marker_data.flags |= RE::MapMarkerData::ShowAllHidden;
    if (!marker.write(marker_data).starts_with("error:unsupported CommonLib hook: MapMarkerData::ShowAllHidden")) return 8;
    if (!marker.refresh(marker_data) || marker_data.flags != 3) return 9;
    RE::ObjectEquipParams params;
    if (equip(actor, item, params) != "ok") return 10;
    params.force_equip = true;
    if (!params.unsupported() || !equip(actor, item, params).starts_with("error:unsupported CommonLib hook: ActorEquipManager::EquipObject")) return 11;
    return 0;
}
"#,
    )
    .unwrap();
    let binary = scratch.join("magic-live");
    let compiled = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(shim.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-13 live contract fixture");
    assert!(
        compiled.status.success(),
        "C++ compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let mut world = fixture_world();
    let actor_global = global(0x20);
    let actor_ref = world.actor(actor_global).unwrap();
    actor_ref.favorites.clear();
    let actor_form = runtime(&world, 0x20);
    let spell_form = runtime(&world, 2);
    let effect_form = runtime(&world, 3);
    let item_form = runtime(&world, 4);
    world.reference(actor_global).unwrap().on_map = false;
    world.reference(actor_global).unwrap().fast_travel = false;

    let mut child = Command::new(binary)
        .args([
            actor_form.to_string(),
            spell_form.to_string(),
            effect_form.to_string(),
            item_form.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut requests = BufReader::new(child.stdout.take().unwrap());
    let mut responses = child.stdin.take().unwrap();
    let mut request = String::new();
    loop {
        request.clear();
        if requests.read_line(&mut request).unwrap() == 0 {
            break;
        }
        let mut words = request.split_whitespace();
        let operation = words.next().unwrap_or_default();
        let actor_id = FormId(words.next().unwrap_or_default().parse().unwrap_or_default());
        let value = words.next().unwrap_or_default();
        let response = {
            let mut allocator = allocate;
            let mut adapter = CommonLibMagic::new(&mut world, &mut allocator);
            match operation {
                "favorite_get" => adapter
                    .is_favorite(actor_id, FormId(value.parse().unwrap_or_default()))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "favorite_set" => adapter
                    .set_favorite(
                        actor_id,
                        FormId(value.parse().unwrap_or_default()),
                        words.next() == Some("true"),
                    )
                    .map(|changed| if changed { "changed" } else { "unchanged" }.to_owned())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "cannot_cast" => adapter
                    .cannot_cast(actor_id, FormId(value.parse().unwrap_or_default()))
                    .map(|reason| (reason as u8).to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "has_effect" => adapter
                    .has_magic_effect(actor_id, FormId(value.parse().unwrap_or_default()))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "effect_count" => adapter
                    .active_effect_count(actor_id)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "dispel" => adapter
                    .dispel_spell(actor_id, FormId(value.parse().unwrap_or_default()))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "marker_get" => adapter
                    .map_marker_flags(actor_id)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "marker_set" => adapter
                    .set_map_marker_flags(actor_id, value.parse().unwrap_or_default())
                    .map(|()| "ok".to_owned())
                    .unwrap_or_else(|e| format!("error:{e}")),
                "equip" => {
                    let item = FormId(value.parse().unwrap_or_default());
                    let supported = words.next() == Some("supported");
                    let params = if supported {
                        ObjectEquipParams::default()
                    } else {
                        ObjectEquipParams {
                            force_equip: true,
                            ..Default::default()
                        }
                    };
                    match adapter.equip_object(actor_id, item, params) {
                        Ok(()) => "ok".to_owned(),
                        Err(UnsupportedState::Hook(_)) if !supported => "error:unsupported CommonLib hook: ActorEquipManager::EquipObject unsupported ObjectEquipParams".to_owned(),
                        Err(error) => format!("error:{error}"),
                    }
                }
                _ => "error:unknown operation".to_owned(),
            }
        };
        writeln!(responses, "{response}").unwrap();
        responses.flush().unwrap();
    }
    assert!(
        child.wait().unwrap().success(),
        "C++ live magic fixture failed"
    );
    assert!(world
        .actor_ref(actor_global)
        .unwrap()
        .favorites
        .contains(&global(2)));
    let reference = world.reference(actor_global).unwrap();
    assert!(reference.on_map && reference.fast_travel);
    assert!(world
        .actor_ref(actor_global)
        .unwrap()
        .equipped
        .contains(&global(4)));
    std::fs::remove_dir_all(scratch).unwrap();
}
