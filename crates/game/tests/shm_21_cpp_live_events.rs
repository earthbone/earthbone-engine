// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use soe_commonlib_shim::events::EventState;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn live_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), 0x802)
            .sub(Tag(*b"MNAM"), 1_u32.to_le_bytes())
            .sub(
                Tag(*b"FNPR"),
                [1_u16.to_le_bytes().as_slice(), &[0, 0]].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x920).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shm21CppLive.esp", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world
            .settings
            .insert("fbleedoutdefault".into(), soe_papyrus::Value::Float(0.2));
        world
            .reference(id(0x910))
            .unwrap()
            .inventory
            .add(id(0x801), 4)
            .unwrap();
        assert!(world.kill_actor(id(0x900), None));
        world.actor(id(0x901)).unwrap().unconscious = true;
        assert!(world.kill_actor(id(0x901), None));
        world.actor(id(0x901)).unwrap().dead = false;
        assert_eq!(
            world.reserve_furniture_marker(id(0x920), id(0x901)),
            Some(0)
        );
        world.reference(id(0x910)).unwrap().deleted = true;
    }
    game
}

#[test]
fn shm_21_cpp_event_contracts_query_live_world_and_reject_hooks() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shim = root.join("../commonlib-shim");
    let scratch = std::env::temp_dir().join(format!("soe-shm-21-live-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("live_events.cpp");
    std::fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cassert>
#include <iostream>
#include <sstream>

std::string ask(const char* operation, std::uint32_t first, std::uint32_t second = 0) {
    std::cout << operation << ' ' << first << ' ' << second << std::endl;
    std::string response;
    if (!std::getline(std::cin, response)) return "missing";
    return response;
}

struct WorldState final : RE::EventState {
    std::optional<std::int32_t> inventory_count(RE::FormId container, RE::FormId item) const override {
        const auto result = ask("count", container.value, item.value);
        if (result == "missing") return std::nullopt;
        return std::stoi(result);
    }
    std::optional<bool> is_dead(RE::ActorHandle actor) const override {
        const auto result = ask("dead", actor.value);
        if (result == "missing") return std::nullopt;
        return result == "1";
    }
    std::optional<bool> is_in_bleedout(RE::ActorHandle actor) const override {
        const auto result = ask("bleedout", actor.value);
        if (result == "missing") return std::nullopt;
        return result == "1";
    }
    std::optional<bool> is_form_deleted(RE::FormId form) const override {
        const auto result = ask("deleted", form.value);
        if (result == "missing") return std::nullopt;
        return result == "1";
    }
    std::optional<RE::TESFurnitureEvent> furniture_event(RE::ActorHandle actor) const override {
        std::istringstream response(ask("furniture", actor.value));
        std::uint32_t actor_id{};
        std::uint32_t furniture_id{};
        std::uint32_t kind{};
        if (!(response >> actor_id >> furniture_id >> kind)) return std::nullopt;
        return RE::TESFurnitureEvent{{actor_id}, {furniture_id},
            static_cast<RE::FurnitureEventType>(kind)};
    }
};

int main() {
    WorldState state;
    const auto count = state.inventory_count({0x910}, {0x801});
    assert(count && *count == 4);
    const RE::TESContainerChangedEvent moved{{}, RE::FormId{0x910}, RE::FormId{0x801}, *count, {0x910}, 7};
    assert(moved.newContainer->value == 0x910 && moved.baseObj.value == 0x801);
    assert(moved.itemCount == 4 && moved.reference.value == 0x910 && moved.uniqueID == 7);

    const auto dead = state.is_dead({0x900});
    assert(dead && *dead);
    const RE::TESDeathEvent death{{0x900}, std::nullopt, *dead};
    assert(death.actorDying.value == 0x900 && !death.actorKiller && death.dead);

    const auto bleeding = state.is_in_bleedout({0x901});
    assert(bleeding && *bleeding);
    const RE::TESEnterBleedoutEvent bleedout{{0x901}};
    assert(bleedout.actor.value == 0x901);

    const auto deleted_state = state.is_form_deleted({0x910});
    assert(deleted_state && *deleted_state);
    const RE::TESFormDeleteEvent deleted{{0x910}};
    assert(deleted.formID.value == 0x910);

    const auto furniture = state.furniture_event({0x901});
    assert(furniture && furniture->actor.value == 0x901);
    assert(furniture->targetFurniture.value == 0x920);
    assert(furniture->type == RE::FurnitureEventType::Enter);
    assert(!state.is_dead({0xffffffff}));
    assert(!state.furniture_event({0xffffffff}));

    const auto hook = RE::request_hook(RE::HookKind::Vtable);
    assert(hook.code == RE::HookErrorCode::Unsupported);
    assert(hook.kind == RE::HookKind::Vtable);
}
"#,
    )
    .unwrap();
    let compiler = std::env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    let compile_output = Command::new(compiler)
        .arg("-std=c++20")
        .arg("-Werror")
        .arg("-I")
        .arg(shim.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(scratch.join("live-events"))
        .output()
        .expect("C++20 compiler is required by SHM-21");
    assert!(
        compile_output.status.success(),
        "C++ live event contract did not compile:\n{}",
        String::from_utf8_lossy(&compile_output.stderr)
    );

    let game = live_game();
    let mut child = Command::new(scratch.join("live-events"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut requests = BufReader::new(child.stdout.take().unwrap());
    let mut responses = child.stdin.take().unwrap();
    let mut line = String::new();
    while requests.read_line(&mut line).unwrap() != 0 {
        let mut fields = line.split_whitespace();
        let operation = fields.next().unwrap_or_default();
        let first = fields.next().unwrap_or_default().parse::<u32>().unwrap();
        let second = fields.next().unwrap_or_default().parse::<u32>().unwrap();
        let mut world = game.world_mut();
        let response = match operation {
            "count" => world
                .inventory_count(
                    soe_commonlib_shim::form::FormId(first),
                    soe_commonlib_shim::form::FormId(second),
                )
                .map_or_else(|| "missing".to_owned(), |value| value.to_string()),
            "dead" => world
                .is_dead(soe_commonlib_shim::refr::ActorHandle(first))
                .map_or_else(|| "missing".to_owned(), |value| u8::from(value).to_string()),
            "bleedout" => world
                .is_in_bleedout(soe_commonlib_shim::refr::ActorHandle(first))
                .map_or_else(|| "missing".to_owned(), |value| u8::from(value).to_string()),
            "deleted" => world
                .is_form_deleted(soe_commonlib_shim::form::FormId(first))
                .map_or_else(|| "missing".to_owned(), |value| u8::from(value).to_string()),
            "furniture" => world
                .furniture_event(soe_commonlib_shim::refr::ActorHandle(first))
                .map_or_else(
                    || "missing".to_owned(),
                    |event| {
                        format!(
                            "{} {} {}",
                            event.actor.0, event.target_furniture.0, event.kind as u32
                        )
                    },
                ),
            _ => panic!("unknown C++ state request: {operation}"),
        };
        writeln!(responses, "{response}").unwrap();
        responses.flush().unwrap();
        line.clear();
    }
    drop(responses);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "C++ live event contract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(scratch).unwrap();
}
