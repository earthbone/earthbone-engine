// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shm_01_generated_headers_match_the_rust_surface_map() {
    let include = crate_root().join("include");
    let generated = soe_commonlib_shim::headers::generated_headers();
    assert_eq!(generated.len(), 13);
    for (relative, expected) in generated {
        let path = include.join(relative);
        let actual = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("generated header {} is missing: {error}", path.display())
        });
        assert_eq!(actual, *expected, "{} is stale", path.display());
    }

    let form = fs::read_to_string(include.join("RE/Form.h")).unwrap();
    for declaration in [
        "struct FormId",
        "struct FormType",
        "class Form",
        "class DataHandler",
    ] {
        assert!(
            form.contains(declaration),
            "missing Rust form API {declaration}"
        );
    }
    let references = fs::read_to_string(include.join("RE/Reference.h")).unwrap();
    for declaration in [
        "struct ObjectRefHandle",
        "struct ActorHandle",
        "struct ActorValue",
        "class ObjectRef",
        "class Actor",
        "add_spell",
        "add_perk",
        "add_item",
    ] {
        assert!(
            references.contains(declaration),
            "missing Rust reference API {declaration}"
        );
    }
    let world = fs::read_to_string(include.join("RE/World.h")).unwrap();
    for declaration in [
        "struct FormKey",
        "struct ProjectileHandle",
        "class PlayerCharacter",
        "class ProcessLists",
        "class Projectile",
    ] {
        assert!(
            world.contains(declaration),
            "missing SHM-14 world API {declaration}"
        );
    }
    let world_types = fs::read_to_string(include.join("RE/WorldTypes.h")).unwrap();
    assert!(world_types.contains("enum class UnsupportedHook"));
    assert!(world_types.contains("ProjectileRuntime") && world_types.contains("BinaryPatch"));
    let math = fs::read_to_string(include.join("RE/Math.h")).unwrap();
    for declaration in [
        "struct NiPoint3",
        "struct NiMatrix3",
        "struct NiTransform",
        "struct SceneNode",
        "class SceneGraph",
    ] {
        assert!(
            math.contains(declaration),
            "missing Rust math API {declaration}"
        );
    }
    let events = fs::read_to_string(include.join("RE/Events.h")).unwrap();
    for declaration in ["NotifyControl", "EventSink", "EventSource", "ScriptEvent"] {
        assert!(
            events.contains(declaration),
            "missing Rust event API {declaration}"
        );
    }
    assert!(events.contains("CellFullyLoaded") && events.contains("SpellsLearned"));
    let utility = fs::read_to_string(include.join("RE/Utility.h")).unwrap();
    for declaration in ["FixedString", "StringCache", "using Array", "ReadWriteLock"] {
        assert!(
            utility.contains(declaration),
            "missing Rust utility API {declaration}"
        );
    }
    let vm = fs::read_to_string(include.join("RE/VirtualMachine.h")).unwrap();
    for declaration in [
        "StaticFunctionTag",
        "VmHandle",
        "VmStackId",
        "NativeDecl",
        "NativeRegistry",
        "register_native",
    ] {
        assert!(
            vm.contains(declaration),
            "missing Rust VM API {declaration}"
        );
    }
    let skse = fs::read_to_string(include.join("SKSE/Interfaces.h")).unwrap();
    for declaration in [
        "class MessagingInterface",
        "class SerializationInterface",
        "class TaskInterface",
        "class PapyrusInterface",
        "TRAMPOLINE",
    ] {
        assert!(
            skse.contains(declaration),
            "missing Rust SKSE API {declaration}"
        );
    }
}

#[test]
fn shm_01_headers_compile_and_match_rust_signatures() {
    let root = crate_root();
    let include = root.join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-01-headers-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("surface.cpp");
    fs::write(
        &source,
        r"
#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <concepts>
#include <type_traits>

static_assert(std::same_as<decltype(&RE::Form::form_id), RE::FormId (RE::Form::*)() const>);
static_assert(std::same_as<decltype(&RE::Form::form_type), RE::FormType (RE::Form::*)() const>);
static_assert(std::same_as<decltype(&RE::Form::editor_id), std::optional<std::string> (RE::Form::*)() const>);
static_assert(std::same_as<decltype(&RE::Form::keywords), std::vector<RE::FormId> (RE::Form::*)() const>);
static_assert(std::same_as<decltype(&RE::FormId::slot), std::optional<std::uint8_t> (RE::FormId::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::FormId::light_slot), std::optional<std::uint16_t> (RE::FormId::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::FormId::local), std::uint32_t (RE::FormId::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::FormId::is_light), bool (RE::FormId::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::FormType::as_str), std::string (RE::FormType::*)() const>);
static_assert(std::same_as<decltype(&RE::DataHandler::lookup), const RE::Form* (RE::DataHandler::*)(RE::FormId) const>);
static_assert(std::same_as<decltype(&RE::DataHandler::lookup_by_editor_id), const RE::Form* (RE::DataHandler::*)(const std::string&) const>);
static_assert(std::same_as<decltype(&RE::DataHandler::forms_of_type), std::vector<RE::FormId> (RE::DataHandler::*)(RE::FormType) const>);
static_assert(std::same_as<decltype(&RE::DataHandler::files), std::vector<std::string> (RE::DataHandler::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::handle), RE::ObjectRefHandle (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::form_id), RE::FormId (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::base), RE::FormId (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::position), RE::NiPoint3 (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::rotation), RE::NiPoint3 (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::parent_cell), std::optional<RE::FormId> (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::is_disabled), bool (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::is_deleted), bool (RE::ObjectRef::*)() const>);
static_assert(std::same_as<decltype(&RE::ObjectRef::set_base), void (RE::ObjectRef::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::Actor::actor_handle), RE::ActorHandle (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::Actor::actor_value), float (RE::Actor::*)(RE::ActorValue) const>);
static_assert(std::same_as<decltype(&RE::Actor::add_item), bool (RE::Actor::*)(RE::FormId, std::uint32_t)>);
static_assert(std::same_as<decltype(&RE::Actor::add_spell), bool (RE::Actor::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::Actor::add_perk), bool (RE::Actor::*)(RE::FormId)>);
static_assert(std::same_as<decltype(&RE::Actor::set_actor_value), void (RE::Actor::*)(RE::ActorValue, float)>);
static_assert(std::same_as<decltype(&RE::Actor::race), RE::FormId (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::Actor::is_dead), bool (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::Actor::is_player), bool (RE::Actor::*)() const>);
static_assert(std::same_as<decltype(&RE::NiPoint3::length), float (RE::NiPoint3::*)() const>);
static_assert(std::same_as<decltype(&RE::NiPoint3::dot), float (RE::NiPoint3::*)(RE::NiPoint3) const>);
static_assert(std::same_as<decltype(&RE::NiPoint3::operator-), RE::NiPoint3 (RE::NiPoint3::*)(RE::NiPoint3) const>);
static_assert(std::same_as<decltype(&RE::NiMatrix3::apply), RE::NiPoint3 (RE::NiMatrix3::*)(RE::NiPoint3) const>);
static_assert(std::same_as<decltype(&RE::NiTransform::apply), RE::NiPoint3 (RE::NiTransform::*)(RE::NiPoint3) const>);
static_assert(std::same_as<decltype(&RE::SceneGraph::find_by_name), std::optional<RE::SceneNode> (RE::SceneGraph::*)(RE::SceneNode, const std::string&) const>);
static_assert(std::same_as<decltype(&RE::SceneGraph::world_transform), RE::NiTransform (RE::SceneGraph::*)(RE::SceneNode) const>);
static_assert(std::same_as<decltype(&RE::SceneGraph::set_local_transform), void (RE::SceneGraph::*)(RE::SceneNode, RE::NiTransform)>);
static_assert(std::same_as<decltype(&RE::EventSink<RE::ScriptEvent>::process), RE::NotifyControl (RE::EventSink<RE::ScriptEvent>::*)(const RE::ScriptEvent&)>);
static_assert(std::same_as<decltype(&RE::EventSource<RE::ScriptEvent>::add_sink), void (RE::EventSource<RE::ScriptEvent>::*)(std::unique_ptr<RE::EventSink<RE::ScriptEvent>>)>);
static_assert(std::same_as<decltype(&RE::EventSource<RE::ScriptEvent>::send), std::size_t (RE::EventSource<RE::ScriptEvent>::*)(const RE::ScriptEvent&)>);
static_assert(std::same_as<decltype(&RE::FixedString::as_str), const std::string& (RE::FixedString::*)() const noexcept>);
static_assert(std::same_as<RE::Array<int>, std::vector<int>>);
static_assert(std::same_as<RE::ReadWriteLock<int>, std::shared_mutex>);
static_assert(std::same_as<decltype(&RE::NativeRegistry::get), const RE::NativeDecl* (RE::NativeRegistry::*)(const std::string&, const std::string&) const>);
static_assert(std::same_as<decltype(&RE::NativeRegistry::duplicates), const std::vector<RE::NativeDecl>& (RE::NativeRegistry::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::NativeRegistry::len), std::size_t (RE::NativeRegistry::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::NativeRegistry::is_empty), bool (RE::NativeRegistry::*)() const noexcept>);
static_assert(std::same_as<decltype(&RE::StringCache::intern), RE::FixedString (RE::StringCache::*)(const std::string&)>);
static_assert(std::same_as<decltype(&RE::NativeRegistry::register_native), bool (RE::NativeRegistry::*)(RE::NativeDecl)>);
static_assert(std::same_as<decltype(&SKSE::MessagingInterface::dispatch), void (SKSE::MessagingInterface::*)(SKSE::MessageKind)>);
static_assert(std::same_as<decltype(&SKSE::MessagingInterface::register_listener), void (SKSE::MessagingInterface::*)(SKSE::PluginHandle, std::function<void(SKSE::MessageKind)>)>);
static_assert(std::same_as<decltype(&SKSE::SerializationInterface::set_unique_id), void (SKSE::SerializationInterface::*)(SKSE::PluginHandle, std::array<std::uint8_t, 4>)>);
static_assert(std::same_as<decltype(&SKSE::SerializationInterface::write_record), bool (SKSE::SerializationInterface::*)(std::array<std::uint8_t, 4>, std::uint32_t, const std::vector<std::uint8_t>&)>);
static_assert(std::same_as<decltype(&SKSE::SerializationInterface::read_record), std::optional<std::tuple<std::array<std::uint8_t, 4>, std::uint32_t, std::vector<std::uint8_t>>> (SKSE::SerializationInterface::*)()>);
static_assert(std::same_as<decltype(&SKSE::SerializationInterface::resolve_form_id), std::optional<RE::FormId> (SKSE::SerializationInterface::*)(RE::FormId) const>);
static_assert(std::same_as<decltype(&SKSE::TaskInterface::add_task), void (SKSE::TaskInterface::*)(std::function<void()>)>);
static_assert(std::same_as<decltype(&SKSE::TaskInterface::add_ui_task), void (SKSE::TaskInterface::*)(std::function<void()>)>);
static_assert(std::same_as<decltype(&SKSE::PapyrusInterface::register_natives), bool (SKSE::PapyrusInterface::*)(std::function<bool(RE::NativeRegistry&)>)>);

struct DeathSink final : RE::EventSink<RE::ScriptEvent> {
    RE::NotifyControl process(const RE::ScriptEvent&) override { return RE::NotifyControl::Stop; }
};

int main() {
    RE::FormId id{0xFE001ABC};
    RE::NiPoint3 position{1.0F, 2.0F, 3.0F};
    RE::EventSource<RE::ScriptEvent> events;
    events.add_sink(std::make_unique<DeathSink>());
    return id.is_light() && position.dot(position) == 14.0F ? 0 : 1;
}
",
    )
    .unwrap();

    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(&include)
        .arg(&source)
        .output()
        .expect("a C++20 compiler is required by the SHM-01 done check");
    assert!(
        output.status.success(),
        "C++ header compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let isolated_source = scratch.join("isolated.cpp");
    for (header, _) in soe_commonlib_shim::headers::generated_headers() {
        fs::write(&isolated_source, format!("#include <{header}>\n")).unwrap();
        let output = Command::new("c++")
            .arg("-std=c++20")
            .arg("-fsyntax-only")
            .arg("-I")
            .arg(&include)
            .arg(&isolated_source)
            .output()
            .expect("a C++20 compiler is required by the SHM-01 done check");
        assert!(
            output.status.success(),
            "C++ header {header} did not compile alone:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(scratch).unwrap();
}
