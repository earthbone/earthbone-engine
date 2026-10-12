// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use soe_commonlib_shim::form::{DataHandler, Form, FormId, FormType};
use soe_commonlib_shim::refr::{Actor, ObjectRef};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn fixture_world() -> (World, GlobalFormId) {
    let base = Record::new(Tag(*b"MISC"), 0x800).zsub(Tag(*b"EDID"), "Shm16Base");
    let actor_base = Record::new(Tag(*b"NPC_"), 0x802).zsub(Tag(*b"EDID"), "Shm16Actor");
    let reference = Record::new(Tag(*b"ACHR"), 0x801)
        .sub(Tag(*b"NAME"), 0x802u32.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            [12.5_f32, -4.0, 9.0, 0.1, 0.2, 0.3]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    let mut regular = Plugin::new(FileHeader::default());
    regular.push_top(base);
    regular.push_top(actor_base);
    regular.push_top(reference);
    let mut light_header = FileHeader::default();
    light_header.flags |= soe_esm::plugin::flags::LIGHT_MASTER;
    let mut light = Plugin::new(light_header);
    light.push_top(Record::new(Tag(*b"MISC"), 0x10).zsub(Tag(*b"EDID"), "LightBase"));
    let mut order = LoadOrder::new();
    order.add("Shm16.esm", regular).unwrap();
    order.add("Shm16Light.esl", light).unwrap();
    let id = GlobalFormId {
        file: 0,
        object: 0x801,
    };
    let mut world = World::new(order);
    let _ = world.reference(id).unwrap();
    world.actor(id).unwrap().weapon_drawn = true;
    (world, id)
}

#[test]
fn shm_16_cpp_calls_populated_world_through_mit_contracts() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../commonlib-shim");
    let scratch = std::env::temp_dir().join(format!("soe-shm-16-cpp-live-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("live.cpp");
    std::fs::write(
        &source,
        r#"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <algorithm>
#include <cassert>
#include <iostream>
#include <sstream>

std::string request(const std::string& value) {
    std::cout << value << std::endl;
    std::string response;
    if (!std::getline(std::cin, response)) return "bridge-error";
    return response;
}
struct LiveForm final : RE::Form {
    RE::FormId id{};
    RE::FormType type{};
    std::string edid;
    RE::FormId form_id() const override { return id; }
    RE::FormType form_type() const override { return type; }
    std::optional<std::string> editor_id() const override { return edid; }
};
struct LiveData final : RE::TESDataHandler {
    mutable LiveForm form;
    const RE::Form* lookup(RE::FormId id) const override {
        std::istringstream response(request("form_id " + std::to_string(id.value)));
        std::string type, edid;
        if (!(response >> form.id.value >> type >> form.edid) || type.size() != 4) return nullptr;
        std::copy(type.begin(), type.end(), form.type.value);
        return form.id == id ? &form : nullptr;
    }
    const RE::Form* lookup_by_editor_id(const std::string& edid) const override {
        std::istringstream response(request("form_editor " + edid));
        std::string type;
        if (!(response >> form.id.value >> type >> form.edid) || type.size() != 4) return nullptr;
        std::copy(type.begin(), type.end(), form.type.value);
        return &form;
    }
    std::vector<RE::FormId> forms_of_type(RE::FormType type) const override {
        std::istringstream response(request("forms_type " + type.as_str()));
        std::vector<RE::FormId> result;
        std::uint32_t id{};
        while (response >> id) result.push_back({id});
        return result;
    }
    std::vector<std::string> files() const override {
        std::istringstream response(request("files"));
        std::vector<std::string> result;
        std::string name;
        while (response >> name) result.push_back(name);
        return result;
    }
    std::optional<RE::FileInfo> file(std::size_t index) const override {
        std::istringstream response(request("file " + std::to_string(index)));
        RE::FileInfo file;
        std::string regular, light;
        if (!(response >> file.name >> regular >> light)) return std::nullopt;
        if (regular != "-") file.regular_index = static_cast<std::uint8_t>(std::stoul(regular));
        if (light != "-") file.light_index = static_cast<std::uint16_t>(std::stoul(light));
        return file;
    }
};
struct LiveRef final : RE::TESObjectREFR {
    mutable std::string unsupported_operation;
    mutable std::uint32_t resolved_pointer_id{};
    RE::ObjectRefHandle handle() const override {
        std::istringstream response(request("ref handle"));
        std::uint32_t handle{};
        response >> handle >> resolved_pointer_id;
        return {handle};
    }
    RE::FormId form_id() const override { return {static_cast<std::uint32_t>(std::stoul(request("ref id")))}; }
    RE::FormId base() const override { return {static_cast<std::uint32_t>(std::stoul(request("ref base")))}; }
    RE::NiPoint3 position() const override {
        std::istringstream r(request("ref position")); float x{}, y{}, z{}; r >> x >> y >> z; return {x, y, z};
    }
    RE::NiPoint3 rotation() const override {
        std::istringstream r(request("ref rotation")); float x{}, y{}, z{}; r >> x >> y >> z; return {x, y, z};
    }
    std::optional<RE::FormId> parent_cell() const override {
        const auto value = request("ref cell");
        return value == "-" ? std::nullopt : std::optional<RE::FormId>{{static_cast<std::uint32_t>(std::stoul(value))}};
    }
    bool is_disabled() const override { return request("ref disabled") == "1"; }
    bool is_deleted() const override { return request("ref deleted") == "1"; }
    RE::Unsupported set_base(RE::FormId id) override { unsupported_operation = request("set_base " + std::to_string(id.value)); return {unsupported_operation.c_str()}; }
};
struct LiveActor final : RE::Actor {
    LiveRef ref;
    RE::ObjectRefHandle handle() const override { return ref.handle(); }
    RE::FormId form_id() const override { return ref.form_id(); }
    RE::FormId base() const override { return ref.base(); }
    RE::NiPoint3 position() const override { return ref.position(); }
    RE::NiPoint3 rotation() const override { return ref.rotation(); }
    std::optional<RE::FormId> parent_cell() const override { return ref.parent_cell(); }
    bool is_disabled() const override { return ref.is_disabled(); }
    bool is_deleted() const override { return ref.is_deleted(); }
    RE::Unsupported set_base(RE::FormId id) override { return ref.set_base(id); }
    RE::ActorHandle actor_handle() const override { return {form_id().value}; }
    RE::ActorPtr actor_ptr() const override { return {{actor_handle()}}; }
    RE::ActorState state() const override { return {false, false, false, weapon_state() == RE::WEAPON_STATE::kDrawn, false}; }
    std::optional<float> actor_value(RE::ActorValue) const override { return std::nullopt; }
    std::optional<RE::ActorApiError> set_actor_value(RE::ActorValue, float) override { return RE::ActorApiError::UnsupportedOperation; }
    RE::FormId race() const override { return {}; }
    bool is_dead() const override { return false; }
    bool is_player() const override { return false; }
    RE::WEAPON_STATE weapon_state() const override { return static_cast<RE::WEAPON_STATE>(std::stoul(request("weapon"))); }
    bool add_spell(RE::FormId) override { return false; }
    bool add_perk(RE::FormId) override { return false; }
    bool add_item(RE::FormId, std::uint32_t) override { return false; }
};
int main() {
    LiveData data;
    const auto* form = data.lookup_by_editor_id("Shm16Base");
    if (!form || form->form_id().value != 0x800 || form->form_type().as_str() != "MISC") return 1;
    if (data.lookup({0x800})->editor_id() != std::optional<std::string>{"Shm16Base"}) return 2;
    if (data.forms_of_type(RE::FormType::new_type("ACHR")) != std::vector<RE::FormId>{{0x801}}) return 3;
    if (data.files() != std::vector<std::string>{"Shm16.esm", "Shm16Light.esl"}) return 4;
    const auto regular = data.file(0), light = data.file(1);
    if (!regular || regular->regular_index != 0 || regular->is_light()) return 5;
    if (!light || light->light_index != 0 || !light->is_light()) return 6;
    LiveRef ref;
    const RE::TESObjectREFRPtr pointer{ref.handle()};
    if (pointer.is_null() || pointer.handle->value != 0x801 || ref.resolved_pointer_id != 0x801 || RE::TESObjectREFRPtr{}.is_null() == false) return 7;
    if (ref.form_id().value != 0x801 || ref.base().value != 0x802) return 8;
    const auto pos = ref.position(), rot = ref.rotation();
    if (pos.x != 12.5F || pos.y != -4.0F || pos.z != 9.0F || rot.x != 0.1F || rot.y != 0.2F || rot.z != 0.3F) return 9;
    if (ref.is_disabled() || ref.is_deleted() || ref.parent_cell()) return 10;
    const auto rejected = ref.set_base({0x900});
    if (std::string(rejected.operation) != "TESObjectREFR::SetBaseObject" || ref.base().value != 0x802) return 11;
    LiveActor actor;
    if (actor.weapon_state() != RE::WEAPON_STATE::kDrawn || !actor.state().weapon_drawn) return 12;
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kSheathed) == 0);
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kWantToDraw) == 1);
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kDrawing) == 2);
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kDrawn) == 3);
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kWantToSheathe) == 4);
    static_assert(static_cast<std::uint32_t>(RE::WEAPON_STATE::kSheathing) == 5);
    if (std::string(SKSE::TRAMPOLINE.what).find("no target") == std::string::npos) return 13;
    return 0;
}
"#,
    )
    .unwrap();
    let binary = scratch.join("shm-16-live");
    let compiled = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-16");
    assert!(
        compiled.status.success(),
        "C++ live-world fixture compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let (mut world, id) = fixture_world();
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
        let response = answer_world_request(&mut world, id, request.trim());
        writeln!(responses, "{response}").unwrap();
        responses.flush().unwrap();
    }
    assert!(
        child.wait().unwrap().success(),
        "C++ rejected live World state"
    );
    std::fs::remove_dir_all(scratch).unwrap();
}

fn answer_world_request(world: &mut World, actor_id: GlobalFormId, request: &str) -> String {
    let mut parts = request.split_whitespace();
    match parts.next().unwrap_or_default() {
        "form_editor" => {
            let handler = world.commonlib_data_handler();
            parts
                .next()
                .and_then(|id| handler.lookup_by_editor_id(id))
                .map_or_else(
                    || "missing".into(),
                    |form| {
                        format!(
                            "{} {} {}",
                            form.form_id().0,
                            form.form_type().as_str(),
                            form.editor_id().unwrap_or_default()
                        )
                    },
                )
        }
        "form_id" => {
            let id = FormId(parts.next().unwrap_or_default().parse().unwrap());
            let handler = world.commonlib_data_handler();
            handler.lookup(id).map_or_else(
                || "missing".into(),
                |form| {
                    format!(
                        "{} {} {}",
                        form.form_id().0,
                        form.form_type().as_str(),
                        form.editor_id().unwrap_or_default()
                    )
                },
            )
        }
        "forms_type" => {
            let signature = parts.next().unwrap_or_default().as_bytes();
            let kind = FormType([signature[0], signature[1], signature[2], signature[3]]);
            world
                .commonlib_data_handler()
                .forms_of_type(kind)
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        }
        "files" => world.commonlib_data_handler().files().join(" "),
        "file" => world
            .commonlib_data_handler()
            .file(parts.next().unwrap_or_default().parse().unwrap())
            .map_or_else(
                || "missing".into(),
                |file| {
                    format!(
                        "{} {} {}",
                        file.name,
                        file.regular_index
                            .map_or_else(|| "-".into(), |value| value.to_string()),
                        file.light_index
                            .map_or_else(|| "-".into(), |value| value.to_string())
                    )
                },
            ),
        "ref" => {
            let field = parts.next().unwrap_or_default();
            if field == "handle" {
                let pointer = { world.commonlib_reference(actor_id).unwrap().pointer() };
                let handle = pointer.0.unwrap();
                let resolved = world
                    .commonlib_reference_ptr(pointer)
                    .map_or(0, |target| target.form_id().0);
                return format!("{} {}", handle.0, resolved);
            }
            let reference = world.commonlib_reference(actor_id).unwrap();
            match field {
                "id" => reference.form_id().0.to_string(),
                "base" => reference.base().0.to_string(),
                "position" => format!(
                    "{} {} {}",
                    reference.position().x,
                    reference.position().y,
                    reference.position().z
                ),
                "rotation" => format!(
                    "{} {} {}",
                    reference.rotation().x,
                    reference.rotation().y,
                    reference.rotation().z
                ),
                "cell" => reference
                    .parent_cell()
                    .map_or_else(|| "-".into(), |id| id.0.to_string()),
                "disabled" => u8::from(reference.is_disabled()).to_string(),
                "deleted" => u8::from(reference.is_deleted()).to_string(),
                _ => "invalid-request".into(),
            }
        }
        "set_base" => world
            .commonlib_reference(actor_id)
            .unwrap()
            .set_base(FormId(parts.next().unwrap_or_default().parse().unwrap()))
            .unwrap_err()
            .operation
            .into(),
        "weapon" => (world.commonlib_actor(actor_id).unwrap().weapon_state() as u32).to_string(),
        _ => "invalid-request".into(),
    }
}
