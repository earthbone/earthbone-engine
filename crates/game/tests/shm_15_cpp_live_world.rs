// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::RuntimeState;
use soe_esm::LoadOrder;
use soe_game::world::World;

#[test]
fn shm_15_cpp_tes_adapter_reads_live_world_and_returns_unsupported_results() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim");
    let scratch = std::env::temp_dir().join(format!("soe-shm-15-live-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("bridge.cpp");
    std::fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <cstdio>
#include <iostream>
std::string request(const std::string& value) {
    std::cout << value << std::endl;
    std::string response;
    if (!std::getline(std::cin, response)) return "bridge-error";
    return response;
}
void copy(char* output, std::size_t size, const std::string& value) {
    std::snprintf(output, size, "%s", value.c_str());
}
bool setting(void*, const char* name, RE::RuntimeSettingRecord* output) {
    const auto value = request(std::string("setting ") + name);
    if (value == "missing") return false;
    output->kind = 2; output->float_value = std::stod(value); return true;
}
float sky_hour(void*) { return std::stof(request("sky")); }
bool reanimate(void*, std::uint64_t id, RE::RuntimeReanimateRecord*) {
    return request("reanimate " + std::to_string(id)) != "missing";
}
void sit_sleep(void*, std::uint32_t actor, RE::RuntimeErrorRecord* output) {
    copy(output->name, sizeof(output->name), "SIT_SLEEP_STATE");
    copy(output->reason, sizeof(output->reason), request("sit_sleep " + std::to_string(actor)));
}
void hook(void*, const char* name, RE::RuntimeErrorRecord* output) {
    const auto reply = request(std::string("hook ") + name);
    const auto split = reply.find('|');
    copy(output->name, sizeof(output->name), reply.substr(0, split));
    copy(output->reason, sizeof(output->reason), reply.substr(split + 1));
}
int main() {
    const RE::RuntimeStateCallbacks callbacks{setting, sky_hour, reanimate, sit_sleep, hook};
    RE::CallbackTES world(nullptr, callbacks);
    const auto setting = world.setting("fTestSetting");
    if (!setting || setting->value.float_value != 2.75F) return 1;
    if (world.setting("missing")) return 2;
    if (world.sky().hour != 18.0F) return 3;
    if (world.reanimate_effect(0xfeed)) return 4;
    RE::SIT_SLEEP_STATE state{};
    RE::UnsupportedWorldHook error{};
    if (world.sit_sleep_state({0x14}, state, error) || std::string(error.name) != "SIT_SLEEP_STATE") return 5;
    const auto unsupported = world.hook("write_vfunc");
    return std::string(unsupported.name) == "binary hook" && std::string(unsupported.reason).find("no target") != std::string::npos ? 0 : 6;
}
"#,
    )
    .unwrap();
    let binary = scratch.join("world-contract");
    let compiled = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-15");
    assert!(
        compiled.status.success(),
        "C++ bridge compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let mut world = World::new(LoadOrder::new());
    world
        .settings
        .insert("ftestsetting".into(), soe_papyrus::Value::Float(2.75));
    world.clock.game_days = 0.75;
    let mut child = Command::new(binary)
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
        let response = match words.next().unwrap_or_default() {
            "setting" => world.setting(words.next().unwrap_or_default()).map_or_else(
                || "missing".to_owned(),
                |value| match value.value {
                    soe_commonlib_shim::world_runtime::SettingValue::Float(value) => {
                        value.to_string()
                    }
                    _ => "missing".to_owned(),
                },
            ),
            "sky" => world.sky().hour.to_string(),
            "reanimate" => world
                .reanimate_effect(words.next().unwrap_or_default().parse().unwrap())
                .map_or_else(|| "missing".to_owned(), |effect| effect.id.to_string()),
            "sit_sleep" => world
                .sit_sleep_state(FormId(words.next().unwrap_or_default().parse().unwrap()))
                .unwrap_err()
                .reason
                .to_owned(),
            "hook" => {
                let value = world.hook(words.next().unwrap_or_default());
                format!("{}|{}", value.name, value.reason)
            }
            _ => "invalid-request".to_owned(),
        };
        writeln!(responses, "{response}").unwrap();
        responses.flush().unwrap();
    }
    assert!(
        child.wait().unwrap().success(),
        "C++ live-world fixture failed"
    );
}
