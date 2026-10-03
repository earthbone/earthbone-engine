// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_runtime::{FormKey, WorldState};
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, LoadOrder, Tag};
use soe_game::world::World;
use soe_game::Game;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn key(file: &str, object: &str) -> FormKey {
    FormKey {
        file: file.parse().unwrap_or(u16::MAX),
        object: object.parse().unwrap_or(u32::MAX),
    }
}

fn serve_world(world: &mut World, reader: &mut impl BufRead, writer: &mut impl Write) {
    let mut request = String::new();
    loop {
        request.clear();
        if reader.read_line(&mut request).expect("read C++ query") == 0 {
            return;
        }
        let fields: Vec<_> = request.split_whitespace().collect();
        let response = match fields.as_slice() {
            ["P"] => WorldState::player(world).map_or_else(
                || "NONE\n".to_owned(),
                |form| {
                    let runtime_id = WorldState::runtime_form_id(world, form).unwrap_or(0);
                    format!("{} {} {}\n", form.file, form.object, runtime_id)
                },
            ),
            ["C"] => format!("{}\n", WorldState::loaded_actors(world).len()),
            ["A", index] => WorldState::loaded_actors(world)
                .get(index.parse::<usize>().unwrap_or(usize::MAX))
                .map_or_else(
                    || "NONE\n".to_owned(),
                    |id| format!("{} {}\n", id.file, id.object),
                ),
            ["X", file, object] => WorldState::reference_position(world, key(file, object))
                .map_or_else(
                    || "NONE\n".to_owned(),
                    |p| format!("{} {} {}\n", p[0], p[1], p[2]),
                ),
            ["V", file, object, name] => WorldState::actor_value(world, key(file, object), name)
                .map_or_else(|| "NONE\n".to_owned(), |value| format!("{value}\n")),
            ["D", file, object] => WorldState::actor_is_dead(world, key(file, object)).map_or_else(
                || "NONE\n".to_owned(),
                |value| format!("{}\n", u8::from(value)),
            ),
            ["R", file, object] => {
                let form = key(file, object);
                WorldState::runtime_form_id(world, form)
                    .map_or_else(|| "NONE\n".to_owned(), |value| format!("{value}\n"))
            }
            _ => "ERROR\n".to_owned(),
        };
        writer
            .write_all(response.as_bytes())
            .expect("write Rust response");
        writer.flush().expect("flush Rust response");
    }
}

#[test]
fn shm_14_cpp_contracts_call_live_world_and_reject_projectile_runtime() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 7).zsub(Tag::new(b"EDID"), "Player"));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).expect("fixture load order");
    let game = Game::new(order);
    let player_id = GlobalFormId {
        file: 0,
        object: 0x14,
    };
    let mut world = game.world_mut();
    assert_eq!(world.player(), player_id);
    world
        .reference(player_id)
        .expect("player reference")
        .position = [10.0, 20.0, 30.0];
    world
        .actor(player_id)
        .expect("player actor")
        .values
        .set_base("health", 75.0)
        .expect("set fixture health");

    let scratch = std::env::temp_dir().join(format!("soe-shm-14-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("scratch directory");
    let source = scratch.join("bridge.cpp");
    let executable = scratch.join("bridge");
    let include =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <concepts>
#include <cstdlib>
#include <iostream>
#include <sstream>
#include <string>
#include <type_traits>
static_assert(sizeof(RE::ObjectRefHandle) == sizeof(std::uint32_t));
static_assert(sizeof(RE::ProjectileHandle) == sizeof(std::uint32_t));
static_assert(std::same_as<decltype(&RE::PlayerCharacter::handle), std::optional<RE::ObjectRefHandle> (RE::PlayerCharacter::*)()>);
static_assert(std::same_as<decltype(&RE::ProcessLists::actor_forms), std::vector<RE::FormKey> (RE::ProcessLists::*)() const>);
static_assert(std::same_as<decltype(&RE::Projectile::runtime), RE::UnsupportedHook (RE::Projectile::*)() const noexcept>);
int rpc(void*, const std::string& request, std::string& response) {
    std::cout << request << '\n' << std::flush;
    return std::getline(std::cin, response) ? 0 : -1;
}
bool read_form(void* context, const std::string& request, RE::FormKey* out) {
    std::string response;
    if (rpc(context, request, response) != 0) return false;
    std::istringstream input(response);
    return static_cast<bool>(input >> out->file >> out->object);
}
bool player(void* context, RE::FormKey* out) {
    std::string response;
    if (rpc(context, "P", response) != 0) return false;
    std::istringstream input(response);
    std::uint32_t runtime_id{};
    return static_cast<bool>(input >> out->file >> out->object >> runtime_id);
}
std::size_t actor_count(void* context) {
    std::string response;
    if (rpc(context, "C", response) != 0) return 0;
    return static_cast<std::size_t>(std::strtoul(response.c_str(), nullptr, 10));
}
bool actor_at(void* context, std::size_t index, RE::FormKey* out) {
    return read_form(context, "A " + std::to_string(index), out);
}
bool position(void* context, RE::FormKey form, RE::Position* out) {
    std::string response;
    const auto query = "X " + std::to_string(form.file) + " " + std::to_string(form.object);
    if (rpc(context, query, response) != 0) return false;
    std::istringstream input(response);
    return static_cast<bool>(input >> out->x >> out->y >> out->z);
}
bool actor_value(void* context, RE::FormKey form, const char* name, float* out) {
    std::string response;
    const auto query = "V " + std::to_string(form.file) + " " + std::to_string(form.object) + " " + name;
    if (rpc(context, query, response) != 0) return false;
    char* end{};
    *out = std::strtof(response.c_str(), &end);
    return end != response.c_str();
}
bool actor_dead(void* context, RE::FormKey form, bool* out) {
    std::string response;
    const auto query = "D " + std::to_string(form.file) + " " + std::to_string(form.object);
    if (rpc(context, query, response) != 0 || response == "NONE") return false;
    *out = response == "1";
    return true;
}
std::uint32_t runtime_id(void* context, RE::FormKey form) {
    std::string response;
    const auto query = "R " + std::to_string(form.file) + " " + std::to_string(form.object);
    if (rpc(context, query, response) != 0 || response == "NONE") return 0;
    return static_cast<std::uint32_t>(std::strtoul(response.c_str(), nullptr, 10));
}
int main() {
    RE::PlayerWorldStateCallbacks callbacks{nullptr, player, actor_count, actor_at,
        position, actor_value, actor_dead, runtime_id};
    RE::CallbackPlayerWorldState provider(callbacks);
    RE::PlayerCharacter player_character(provider);
    RE::ProcessLists processes(provider);
    RE::Projectile projectile(RE::ProjectileHandle{41});
    const auto key = player_character.form_key();
    const auto handle = player_character.handle();
    const auto player_position = player_character.position();
    const auto actors = processes.actor_forms();
    const auto dead = key ? provider.actor_is_dead(*key) : std::nullopt;
    const bool valid = key && key->file == 0 && key->object == 0x14 && handle &&
        handle->value == provider.runtime_form_id(*key) && player_position &&
        player_position->x == 10.0F && player_position->y == 20.0F && player_position->z == 30.0F &&
        player_character.actor_value("health") == 75.0F && dead == false &&
        actors.size() == 1 && actors[0].object == 0x14 && projectile.handle().value == 41 &&
        projectile.runtime() == RE::UnsupportedHook::ProjectileRuntime;
    return valid ? 0 : 1;
}
"#,
    )
    .expect("write C++ bridge source");
    let compile = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C++20 compiler required for SHM-14");
    assert!(
        compile.status.success(),
        "C++ bridge compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let mut child = Command::new(&executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start C++ contract fixture");
    let child_stdout = child.stdout.take().expect("C++ query stream");
    let mut child_stdin = child.stdin.take().expect("Rust response stream");
    let mut reader = BufReader::new(child_stdout);
    serve_world(&mut world, &mut reader, &mut child_stdin);
    drop(child_stdin);
    let status = child.wait().expect("wait for C++ contract fixture");
    assert!(status.success(), "C++ contract rejected live World state");
    drop(world);
    fs::remove_dir_all(scratch).expect("remove scratch directory");
}
