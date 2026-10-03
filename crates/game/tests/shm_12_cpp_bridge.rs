// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{commonlib, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn bridge_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x100));
    plugin.push_top(Record::new(T(b"REFR"), 0x200).sub(T(b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("ShimBridgeFixture.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().add_item(id(0x200), id(0x100), 5);
    game
}

fn answer(game: &mut Game, request: &str) -> String {
    let mut fields = request.split_whitespace();
    match fields.next() {
        Some("FACTORY") => {
            let signature = fields.next().unwrap().as_bytes();
            let signature: [u8; 4] = signature.try_into().unwrap();
            commonlib::form_factory(&game.world(), signature)
                .map(|factory| format!("1|{}", factory.get_form_name()))
                .unwrap_or_else(|_| "0|".to_owned())
        }
        Some("INVENTORY") => {
            let owner = u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let changes = commonlib::inventory_changes(&mut game.world_mut(), id(owner)).unwrap();
            let entries = changes
                .entries
                .iter()
                .map(|entry| {
                    format!(
                        "{}|{}|{}|{}",
                        entry.object.0,
                        entry.count_delta,
                        u8::from(entry.worn),
                        entry.extra_types.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("|");
            format!("{}|{}|{}", changes.owner.0, changes.entries.len(), entries)
        }
        Some("LOOKUP") => {
            let handle = u32::from_str_radix(fields.next().unwrap(), 16).unwrap();
            commonlib::lookup_reference_by_handle(
                &mut game.world_mut(),
                soe_commonlib_shim::form::FormId(handle),
            )
            .map(|reference| {
                let kind = match reference.kind {
                    soe_commonlib_shim::world::ReferenceKind::Actor => "Actor",
                    soe_commonlib_shim::world::ReferenceKind::Reference => "Reference",
                };
                format!("1|{}|{kind}", reference.form_id.0)
            })
            .unwrap_or_else(|_| "0|0|".to_owned())
        }
        _ => "ERR".to_owned(),
    }
}

#[test]
fn shm_12_cpp_contract_calls_live_game_adapter_and_rejects_unsupported_hooks() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let include = root.join("../commonlib-shim/include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-12-bridge-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("live_bridge.cpp");
    std::fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <iostream>
#include <sstream>
#include <string>

std::string request(const std::string& value) {
    std::cout << value << std::endl;
    std::string result;
    std::getline(std::cin, result);
    return result;
}

int main() {
    auto factory_reply = request("FACTORY MISC");
    std::istringstream factory_fields(factory_reply);
    int factory_found = 0;
    std::string factory_name;
    factory_fields >> factory_found;
    std::getline(factory_fields, factory_name, '|');
    std::getline(factory_fields, factory_name);
    if (!factory_found || factory_name != "MISC") return 1;
    RE::IFormFactory factory{factory_name, RE::FormType::new_type("MISC")};
    if (factory.GetFormType().as_str() != "MISC") return 2;
    if (factory.Create() != RE::UnsupportedHook::FormCreation) return 3;

    auto inventory_reply = request("INVENTORY 00000200");
    std::istringstream inventory_fields(inventory_reply);
    std::string field;
    std::getline(inventory_fields, field, '|');
    RE::InventoryChanges changes{{static_cast<std::uint32_t>(std::stoul(field))}, {}, {}};
    std::getline(inventory_fields, field, '|');
    const auto entry_count = std::stoul(field);
    for (std::size_t index = 0; index < entry_count; ++index) {
        std::getline(inventory_fields, field, '|');
        RE::FormId object{static_cast<std::uint32_t>(std::stoul(field))};
        std::getline(inventory_fields, field, '|');
        const auto count = std::stoi(field);
        std::getline(inventory_fields, field, '|');
        const bool worn = std::stoi(field) != 0;
        std::getline(inventory_fields, field, '|');
        const auto extra_count = std::stoul(field);
        changes.entries.push_back({object, count, std::vector<std::uint8_t>(extra_count), worn});
        changes.totals.emplace_back(object, count);
    }
    if (changes.owner.value != 0x200 || changes.GetItemCount({0x100}) != 5) return 4;
    if (changes.entries.size() != 1 || changes.entries.front().count_delta != 5) return 5;
    if (changes.AddEntryData({{0x100}, 1, {}, false}) != RE::UnsupportedHook::InventoryMutation) return 6;
    RE::ItemList items{changes.entries};
    if (items.items.size() != 1 || items.GetSelectedItem() != RE::UnsupportedHook::ItemSelection) return 7;
    if (items.Update() != RE::UnsupportedHook::ItemListUpdate) return 8;

    auto lookup_reply = request("LOOKUP 00000200");
    std::istringstream lookup_fields(lookup_reply);
    int lookup_found = 0;
    std::uint32_t form_id = 0;
    std::string kind;
    char separator = 0;
    lookup_fields >> lookup_found >> separator >> form_id >> separator;
    std::getline(lookup_fields, kind);
    if (!lookup_found || form_id != 0x200 || kind != "Reference") return 9;
    return 0;
}
"#,
    )
    .unwrap();
    let binary = scratch.join("live_bridge");
    let compile = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by the SHM-12 bridge check");
    assert!(
        compile.status.success(),
        "C++ live bridge failed to compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let mut output = BufReader::new(output);
    let mut game = bridge_game();
    let mut request_line = String::new();
    while output.read_line(&mut request_line).unwrap() != 0 {
        let response = answer(&mut game, request_line.trim());
        writeln!(input, "{response}").unwrap();
        input.flush().unwrap();
        request_line.clear();
    }
    drop(input);
    let result = child.wait().unwrap();
    assert!(result.success(), "C++ live bridge failed with {result}");
    std::fs::remove_dir_all(scratch).unwrap();
}
