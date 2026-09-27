// Generated from crates/commonlib-shim/src/skse.rs. Do not edit.
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
