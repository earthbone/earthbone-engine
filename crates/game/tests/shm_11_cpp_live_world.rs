// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_types::unsupported_hook;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, LoadOrder, Tag};
use soe_game::combat::MeleeHit;
use soe_game::world::World;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const ATTACKER: u32 = 0x900;
const TARGET: u32 = 0x901;
const ITEM: u32 = 0xA00;
const HAZARD_BASE: u32 = 0x200;
const HAZARD_REF: u32 = 0x300;

fn form(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn serve_live_world(world: &mut World, reader: &mut impl BufRead, writer: &mut impl Write) {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).expect("read C++ request") == 0 {
            return;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let response = match fields.as_slice() {
            ["W", actor, item] => world
                .commonlib_extra_worn(form(actor.parse().unwrap()), form(item.parse().unwrap()))
                .map_or_else(
                    || "NONE\n".to_owned(),
                    |value| format!("{} {}\n", u8::from(value.worn), u8::from(value.worn_left)),
                ),
            ["S", name] => world.commonlib_game_setting(name).map_or_else(
                || "NONE\n".to_owned(),
                |value| match value {
                    soe_commonlib_shim::world_types::SettingValue::Float(value) => {
                        format!("F {value}\n")
                    }
                    _ => "OTHER\n".to_owned(),
                },
            ),
            ["Z", reference] => world
                .commonlib_hazard(form(reference.parse().unwrap()))
                .map_or_else(
                    || "NONE\n".to_owned(),
                    |value| {
                        format!(
                            "{} {} {} {} {} {}\n",
                            value.reference.0,
                            value.base.0,
                            value.position.x,
                            value.position.y,
                            value.position.z,
                            u8::from(value.enabled)
                        )
                    },
                ),
            ["P", actor] => world
                .commonlib_high_process(form(actor.parse().unwrap()))
                .map_or_else(
                    || "NONE\n".to_owned(),
                    |value| {
                        format!(
                            "{} {} {} {}\n",
                            value.actor.0,
                            u8::from(value.in_combat),
                            value.combat_target.map_or(0, |id| id.0),
                            u8::from(value.staggered)
                        )
                    },
                ),
            ["H", attacker, target, weapon] => {
                let hit = MeleeHit {
                    attacker: form(attacker.parse().unwrap()),
                    target: form(target.parse().unwrap()),
                    weapon: form(weapon.parse().unwrap()),
                    is_bash: false,
                    is_power_attack: false,
                };
                let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(0);
                world
                    .commonlib_resolve_hit(hit, Vec::new(), Vec::new(), &mut allocate)
                    .map_or_else(
                        |error| format!("ERROR {error}\n"),
                        |value| {
                            format!(
                                "{} {} {} {} {} {} {} {}\n",
                                value.attacker.0,
                                value.target.0,
                                value.weapon.0,
                                value.weapon_damage,
                                value.armor_reduction,
                                value.health_damage,
                                u8::from(value.blocked),
                                u8::from(value.killed)
                            )
                        },
                    )
            }
            ["U", type_name, hook_name] => {
                let result = unsupported_hook(type_name, hook_name);
                format!(
                    "{} {} {}\n",
                    result.type_name, result.hook_name, result.reason
                )
            }
            _ => "ERROR\n".to_owned(),
        };
        writer
            .write_all(response.as_bytes())
            .expect("write C++ response");
        writer.flush().expect("flush C++ response");
    }
}

#[test]
fn shm_11_cpp_contract_reads_live_world_types_and_rejects_unsupported_hooks() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"HAZD"), HAZARD_BASE));
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x800));
    for actor in [ATTACKER, TARGET] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor).sub(Tag::new(b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), HAZARD_REF)
            .sub(Tag::new(b"NAME"), HAZARD_BASE.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    let mut weapon_data = vec![0; 10];
    weapon_data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    weapon_data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut weapon_stats = vec![0; 100];
    weapon_stats[0] = 1;
    weapon_stats[4..8].copy_from_slice(&1_f32.to_bits().to_le_bytes());
    weapon_stats[8..12].copy_from_slice(&1.1_f32.to_bits().to_le_bytes());
    weapon_stats[76..80].copy_from_slice(&6_u32.to_le_bytes());
    weapon_stats[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), ITEM)
            .sub(Tag::new(b"DATA"), weapon_data)
            .sub(Tag::new(b"DNAM"), weapon_stats),
    );
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).expect("fixture load order");
    let mut world = World::new(order);
    world.settings.insert(
        "fcombatdistance".to_owned(),
        soe_papyrus::Value::Float(96.0),
    );
    world
        .reference(form(ATTACKER))
        .expect("attacker reference")
        .inventory
        .restore(
            form(ITEM),
            1,
            vec![soe_game::inventory::Stack::new(
                1,
                vec![soe_game::inventory::Extra {
                    kind: 22,
                    fields: Vec::new(),
                }],
            )
            .unwrap()],
        )
        .unwrap();
    world
        .reference(form(HAZARD_REF))
        .expect("hazard reference")
        .position = [4.0, 5.0, 6.0];
    world
        .reference(form(HAZARD_REF))
        .expect("hazard reference")
        .enabled = false;
    let stagger_end_time = world.clock.real_seconds + 1.0;
    {
        let actor = world.actor(form(ATTACKER)).expect("attacker actor");
        actor.in_combat = true;
        actor.combat_target = Some(form(TARGET));
        actor.stagger_end_time = Some(stagger_end_time);
        actor.values.set_base("onehanded", 100.0).unwrap();
    }
    world.actor(form(TARGET)).expect("target actor");
    world
        .set_actor_value(form(TARGET), "health", 100.0)
        .unwrap();
    world.add_item(form(ATTACKER), form(ITEM), 1);
    world
        .actor(form(ATTACKER))
        .unwrap()
        .equipped
        .insert(form(ITEM));

    let scratch = std::env::temp_dir().join(format!("soe-shm-11-live-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("scratch directory");
    let source = scratch.join("world_types.cpp");
    let binary = scratch.join("world_types");
    let include =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim/include");
    fs::write(&source, r#"
#include <RE/Skyrim.h>
#include <cassert>
#include <iostream>
#include <sstream>
int rpc(const std::string& request, std::string& response) {
    std::cout << request << '\n' << std::flush;
    return std::getline(std::cin, response) ? 0 : -1;
}
struct Live final : RE::WorldTypes, RE::GameSettingCollection {
    std::optional<RE::SettingValue> get_setting(const std::string& name) const override {
        std::string response;
        if (rpc("S " + name, response) || response == "NONE") return std::nullopt;
        std::istringstream input(response); char kind{}; RE::SettingValue value{};
        input >> kind >> value.float_value; value.kind = RE::SettingValue::Kind::Float; return value;
    }
    std::optional<RE::WornState> extra_worn(RE::FormId actor, RE::FormId item) const override {
        std::string response;
        if (rpc("W " + std::to_string(actor.value) + " " + std::to_string(item.value), response) || response == "NONE") return std::nullopt;
        std::istringstream input(response); RE::WornState value{}; input >> value.worn >> value.worn_left; return value;
    }
    std::optional<RE::HazardState> hazard(RE::FormId reference) const override {
        std::string response;
        if (rpc("Z " + std::to_string(reference.value), response) || response == "NONE") return std::nullopt;
        RE::HazardState value{}; std::istringstream input(response);
        input >> value.reference.value >> value.base.value >> value.position.x >> value.position.y >> value.position.z >> value.enabled;
        return value;
    }
    std::optional<RE::HighProcessState> high_process(RE::FormId actor) const override {
        std::string response;
        if (rpc("P " + std::to_string(actor.value), response) || response == "NONE") return std::nullopt;
        RE::HighProcessState value{}; std::uint32_t target{}; std::istringstream input(response);
        input >> value.actor.value >> value.in_combat >> target >> value.staggered;
        if (target) value.combat_target = RE::FormId{target}; return value;
    }
    std::optional<RE::HitData> resolve_hit(RE::FormId attacker, RE::FormId target, RE::FormId weapon) override {
        std::string response;
        if (rpc("H " + std::to_string(attacker.value) + " " + std::to_string(target.value) + " " + std::to_string(weapon.value), response)) return std::nullopt;
        RE::HitData value{}; std::istringstream input(response);
        input >> value.attacker.value >> value.target.value >> value.weapon.value >> value.weapon_damage >> value.armor_reduction >> value.health_damage >> value.blocked >> value.killed;
        return input ? std::optional<RE::HitData>{value} : std::nullopt;
    }
    RE::UnsupportedHookInfo unsupported_hook(const std::string& type, const std::string& hook) const override {
        std::string response; rpc("U " + type + " " + hook, response);
        std::istringstream input(response); RE::UnsupportedHookInfo value{};
        input >> value.type_name >> value.hook_name; std::getline(input >> std::ws, value.reason); return value;
    }
};
int main() {
    Live world;
    const auto worn = world.extra_worn({0x900}, {0xA00});
    const auto setting = world.get_setting("fCombatDistance");
    const auto hazard = world.hazard({0x300});
    const auto process = world.high_process({0x900});
    const auto hit = world.resolve_hit({0x900}, {0x901}, {0xA00});
    assert(worn && worn->worn && !worn->worn_left);
    assert(setting && setting->kind == RE::SettingValue::Kind::Float && setting->float_value == 96.0F);
    assert(hazard && hazard->reference.value == 0x300 && hazard->base.value == 0x200 && hazard->position.x == 4.0F && hazard->position.z == 6.0F && !hazard->enabled);
    assert(process && process->in_combat && process->staggered && process->combat_target->value == 0x901);
    assert(hit && hit->attacker.value == 0x900 && hit->target.value == 0x901 && hit->weapon.value == 0xA00 && hit->weapon_damage > 0.0F && hit->health_damage == hit->weapon_damage - hit->armor_reduction && !hit->blocked && !hit->killed);
    const auto reject = [&world](const char* type, const char* hook) { auto value = world.unsupported_hook(type, hook); assert(value.type_name == type && value.hook_name == hook && !value.reason.empty()); };
    reject("ExtraWorn", "attach"); reject("GameSettingCollection", "singleton"); reject("GameSettingCollection", "write");
    reject("Hazard", "tick"); reject("Hazard", "finish_load"); reject("HighProcessData", "headtrack"); reject("HighProcessData", "animation");
    reject("HitData", "dispatch"); reject("HitData", "relocation");
}
"#).expect("write C++ live fixture");
    let compile = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("C++20 compiler required for SHM-11");
    assert!(
        compile.status.success(),
        "C++ fixture compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start C++ live fixture");
    let mut requests = BufReader::new(child.stdout.take().expect("C++ query stream"));
    let mut responses = child.stdin.take().expect("Rust response stream");
    serve_live_world(&mut world, &mut requests, &mut responses);
    drop(responses);
    let status = child.wait().expect("wait for C++ live fixture");
    assert!(status.success(), "C++ contract rejected live World state");
    fs::remove_dir_all(scratch).expect("remove scratch directory");
}
