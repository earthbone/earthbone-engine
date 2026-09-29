// SPDX-License-Identifier: MIT

//! Generated C++ declarations for the Rust shim surface.
//!
//! The declarations are kept here beside the Rust API they project. Run
//! `cargo run -p soe-commonlib-shim --bin generate-headers` after changing
//! this map. The generated headers are a source-level API and do not provide
//! an FFI bridge to a running engine.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RE_FORM: &str = r"// Generated from crates/commonlib-shim/src/form.rs. Do not edit.
#pragma once
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
struct FormId {
    std::uint32_t value;
    static constexpr std::uint32_t LIGHT_PREFIX = 0xFE;
    static constexpr std::uint32_t LIGHT_LOCAL_MASK = 0x0FFF;
    std::optional<std::uint8_t> slot() const noexcept {
        const auto top = static_cast<std::uint8_t>(value >> 24);
        return top == LIGHT_PREFIX ? std::nullopt : std::optional<std::uint8_t>{top};
    }
    std::optional<std::uint16_t> light_slot() const noexcept {
        return (value >> 24) == LIGHT_PREFIX
            ? std::optional<std::uint16_t>{static_cast<std::uint16_t>((value >> 12) & 0x0FFF)}
            : std::nullopt;
    }
    std::uint32_t local() const noexcept {
        return light_slot() ? value & LIGHT_LOCAL_MASK : value & 0x00FFFFFF;
    }
    bool is_light() const noexcept { return light_slot().has_value(); }
};

struct FormType {
    char value[4];
    static constexpr FormType new_type(const char (&sig)[5]) noexcept {
        return {{sig[0], sig[1], sig[2], sig[3]}};
    }
    std::string as_str() const { return std::string(value, 4); }
};

class Form {
public:
    virtual ~Form() = default;
    virtual FormId form_id() const = 0;
    virtual FormType form_type() const = 0;
    virtual std::optional<std::string> editor_id() const { return std::nullopt; }
    virtual std::vector<FormId> keywords() const { return {}; }
};

class DataHandler {
public:
    virtual ~DataHandler() = default;
    virtual const Form* lookup(FormId id) const = 0;
    virtual const Form* lookup_by_editor_id(const std::string& editor_id) const = 0;
    virtual std::vector<FormId> forms_of_type(FormType kind) const = 0;
    virtual std::vector<std::string> files() const = 0;
};
} // namespace RE
";

const RE_REFERENCE: &str = r#"// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
#pragma once
#include "RE/Math.h"
#include "RE/Form.h"
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct ObjectRefHandle { std::uint32_t value; };
struct ActorHandle {
    std::uint32_t value;
    constexpr operator ObjectRefHandle() const noexcept { return {value}; }
};
struct ActorValue {
    std::uint32_t value;
    static const ActorValue HEALTH;
    static const ActorValue MAGICKA;
    static const ActorValue STAMINA;
};
inline const ActorValue ActorValue::HEALTH{24};
inline const ActorValue ActorValue::MAGICKA{25};
inline const ActorValue ActorValue::STAMINA{26};

class ObjectRef {
public:
    virtual ~ObjectRef() = default;
    virtual ObjectRefHandle handle() const = 0;
    virtual FormId form_id() const = 0;
    virtual FormId base() const = 0;
    virtual NiPoint3 position() const = 0;
    virtual NiPoint3 rotation() const = 0;
    virtual std::optional<FormId> parent_cell() const = 0;
    virtual bool is_disabled() const = 0;
    virtual bool is_deleted() const = 0;
    virtual void set_base(FormId base) = 0;
};

class Actor : public ObjectRef {
public:
    virtual ActorHandle actor_handle() const = 0;
    virtual float actor_value(ActorValue av) const = 0;
    virtual void set_actor_value(ActorValue av, float value) = 0;
    virtual FormId race() const = 0;
    virtual bool is_dead() const = 0;
    virtual bool is_player() const = 0;
    virtual bool add_spell(FormId spell) = 0;
    virtual bool add_perk(FormId perk) = 0;
    virtual bool add_item(FormId item, std::uint32_t count) = 0;
};
} // namespace RE
"#;

const RE_MATH: &str = r"// Generated from crates/commonlib-shim/src/ni.rs. Do not edit.
#pragma once
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct NiPoint3 {
    float x{};
    float y{};
    float z{};
    constexpr NiPoint3() = default;
    constexpr NiPoint3(float x_value, float y_value, float z_value) : x(x_value), y(y_value), z(z_value) {}
    float length() const;
    float dot(NiPoint3 other) const;
    constexpr NiPoint3 operator-(NiPoint3 other) const {
        return {x - other.x, y - other.y, z - other.z};
    }
};

struct NiMatrix3 {
    float value[3][3];
    NiMatrix3() : value{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}} {}
    NiPoint3 apply(NiPoint3 point) const;
};

struct NiTransform {
    NiMatrix3 rotate{};
    NiPoint3 translate{};
    float scale{1.0F};
    NiPoint3 apply(NiPoint3 point) const;
};

struct SceneNode { std::uint64_t value; };
class SceneGraph {
public:
    virtual ~SceneGraph() = default;
    virtual std::optional<SceneNode> find_by_name(SceneNode root, const std::string& name) const = 0;
    virtual NiTransform world_transform(SceneNode node) const = 0;
    virtual void set_local_transform(SceneNode node, NiTransform transform) = 0;
};
} // namespace RE
";

const RE_WORLD: &str = r#"// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <array>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
enum class UnsupportedHook { ActivateReferenceChildren };
class Calendar {
public:
    virtual ~Calendar() = default;
    virtual std::uint32_t year() const = 0;
    virtual std::uint32_t month() const = 0;
    virtual std::uint32_t day() const = 0;
    virtual double hour() const = 0;
    virtual double days_passed() const = 0;
    virtual double timescale() const = 0;
};
class Character {
public:
    virtual ~Character() = default;
    virtual FormId form_id() const = 0;
    virtual std::optional<float> actor_value(const std::string& name) const = 0;
    virtual bool is_dead() const = 0;
    virtual std::optional<FormId> race() const = 0;
};
class DEFAULT_OBJECTS {
public:
    virtual ~DEFAULT_OBJECTS() = default;
    struct Result { bool success; std::optional<FormId> form; std::string error; };
    virtual Result get(const std::array<char, 4>& key) const = 0;
};
struct AliasInstance { FormId quest; std::uint32_t alias_id; std::optional<FormId> reference; std::vector<FormId> packages; };
class ExtraAliasInstanceArray {
public:
    virtual ~ExtraAliasInstanceArray() = default;
    virtual std::vector<AliasInstance> instances(FormId reference) const = 0;
};
class ExtraActivateRefChildren {
public:
    virtual ~ExtraActivateRefChildren() = default;
    struct ChildrenResult { bool success; std::vector<FormId> children; UnsupportedHook error; };
    struct TimerResult { bool success; float seconds; UnsupportedHook error; };
    virtual ChildrenResult children(FormId reference) const = 0;
    virtual TimerResult timer(FormId reference) const = 0;
    virtual std::optional<UnsupportedHook> activate_children(FormId reference) const = 0;
};
} // namespace RE
"#;

const RE_EVENTS: &str = r"// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
#pragma once
#include <cstddef>
#include <memory>
#include <utility>
#include <vector>

namespace RE {
enum class NotifyControl { Continue, Stop };

template <class Event>
class EventSink {
public:
    virtual ~EventSink() = default;
    virtual NotifyControl process(const Event& event) = 0;
};

template <class Event>
class EventSource {
public:
    void add_sink(std::unique_ptr<EventSink<Event>> sink) { sinks_.push_back(std::move(sink)); }
    std::size_t send(const Event& event) {
        std::size_t seen = 0;
        for (const auto& sink : sinks_) {
            ++seen;
            if (sink->process(event) == NotifyControl::Stop) break;
        }
        return seen;
    }
private:
    std::vector<std::unique_ptr<EventSink<Event>>> sinks_;
};

enum class ScriptEvent {
    CellFullyLoaded, ContainerChanged, Death, EnterBleedout, FormDelete,
    Furniture, GrabRelease, Harvested, Hit, ObjectLoaded, QuestStage,
    QuestStartStop, Disarmed, MenuOpenClose, CrosshairRef, ActorKill,
    CriticalHit, DragonSoulsGained, LevelIncrease, LocationDiscovery,
    ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned
};
} // namespace RE
";

const RE_UTILITY: &str = r"// Generated from crates/commonlib-shim/src/util.rs. Do not edit.
#pragma once
#include <memory>
#include <shared_mutex>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace RE {
class FixedString {
public:
    const std::string& as_str() const noexcept { return *value_; }
    friend bool operator==(const FixedString& left, const FixedString& right) noexcept {
        return left.value_ == right.value_;
    }
private:
    explicit FixedString(std::shared_ptr<const std::string> value) : value_(std::move(value)) {}
    std::shared_ptr<const std::string> value_;
    friend class StringCache;
};

class StringCache {
public:
    FixedString intern(const std::string& value);
private:
    std::shared_mutex mutex_;
    std::unordered_map<std::string, std::shared_ptr<const std::string>> table_;
};

template <class T> using Array = std::vector<T>;
template <class T> using ReadWriteLock = std::shared_mutex;
} // namespace RE
";

const RE_VM: &str = r"// Generated from crates/commonlib-shim/src/vm.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <tuple>
#include <vector>

namespace RE {
struct StaticFunctionTag {};
struct VmHandle { std::uint64_t value; };
struct VmStackId { std::uint32_t value; };
struct NativeDecl { std::string class_name; std::string function; bool latent; };

class NativeRegistry {
public:
    // Rust names this method `register`, which is a C++ keyword.
    bool register_native(NativeDecl declaration);
    const NativeDecl* get(const std::string& class_name, const std::string& function) const;
    const std::vector<NativeDecl>& duplicates() const noexcept;
    std::size_t len() const noexcept;
    bool is_empty() const noexcept;
private:
    std::vector<NativeDecl> natives_;
    std::vector<NativeDecl> duplicates_;
};
} // namespace RE
";

const SKSE_INTERFACES: &str = r#"// Generated from crates/commonlib-shim/src/skse.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/VirtualMachine.h"
#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace SKSE {
struct PluginHandle {
    std::uint32_t value;
    static const PluginHandle INVALID;
};
inline const PluginHandle PluginHandle::INVALID{0xFFFFFFFF};
struct PluginDeclaration { std::string name; std::uint32_t version; std::string author; };
enum class MessageKind { PostLoad, PostPostLoad, DataLoaded, NewGame, PreLoadGame, PostLoadGame, SaveGame, DeleteGame };

class MessagingInterface {
public:
    virtual ~MessagingInterface() = default;
    virtual void register_listener(PluginHandle plugin, std::function<void(MessageKind)> listener) = 0;
    virtual void dispatch(MessageKind kind) = 0;
};
class SerializationInterface {
public:
    virtual ~SerializationInterface() = default;
    virtual void set_unique_id(PluginHandle plugin, std::array<std::uint8_t, 4> id) = 0;
    virtual bool write_record(std::array<std::uint8_t, 4> kind, std::uint32_t version,
                              const std::vector<std::uint8_t>& data) = 0;
    virtual std::optional<std::tuple<std::array<std::uint8_t, 4>, std::uint32_t,
                                     std::vector<std::uint8_t>>> read_record() = 0;
    virtual std::optional<RE::FormId> resolve_form_id(RE::FormId saved) const = 0;
};
class TaskInterface {
public:
    virtual ~TaskInterface() = default;
    virtual void add_task(std::function<void()> task) = 0;
    virtual void add_ui_task(std::function<void()> task) = 0;
};
class PapyrusInterface {
public:
    virtual ~PapyrusInterface() = default;
    virtual bool register_natives(std::function<bool(RE::NativeRegistry&)> callback) = 0;
};
struct Unsupported { const char* what; };
inline constexpr Unsupported TRAMPOLINE{
    "SKSE::Trampoline patches machine code and has no target in this engine"};
} // namespace SKSE
"#;

const FILES: &[(&str, &str)] = &[
    ("RE/Form.h", RE_FORM),
    ("RE/Reference.h", RE_REFERENCE),
    ("RE/Math.h", RE_MATH),
    ("RE/World.h", RE_WORLD),
    ("RE/Events.h", RE_EVENTS),
    ("RE/Utility.h", RE_UTILITY),
    ("RE/VirtualMachine.h", RE_VM),
    ("RE/Skyrim.h", "// Generated umbrella for the supported RE surface. Do not edit.\n#pragma once\n#include \"RE/Form.h\"\n#include \"RE/Reference.h\"\n#include \"RE/World.h\"\n#include \"RE/Events.h\"\n#include \"RE/Utility.h\"\n#include \"RE/VirtualMachine.h\"\n"),
    ("SKSE/Interfaces.h", SKSE_INTERFACES),
    ("SKSE/SKSE.h", "// Generated umbrella for the supported SKSE surface. Do not edit.\n#pragma once\n#include \"SKSE/Interfaces.h\"\n"),
];

/// Write every generated header below `root` and return its relative paths.
pub fn write_headers(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(FILES.len());
    for (relative, contents) in FILES {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        written.push(PathBuf::from(relative));
    }
    Ok(written)
}

/// Paths and contents in stable order, used by the generation check.
pub fn generated_headers() -> &'static [(&'static str, &'static str)] {
    FILES
}
