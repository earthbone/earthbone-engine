// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Reference.h"
#include "RE/WorldTypes.h"
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace RE {
struct FormKey { std::uint16_t file; std::uint32_t object; };
struct ProjectileHandle { std::uint32_t value; };
struct Position { float x; float y; float z; };
class WorldState {
public:
    virtual ~WorldState() = default;
    virtual std::optional<FormKey> player() = 0;
    virtual std::vector<FormKey> loaded_actors() const = 0;
    virtual std::optional<Position> reference_position(FormKey form) = 0;
    virtual std::optional<float> actor_value(FormKey form, const char* name) = 0;
    virtual std::optional<bool> actor_is_dead(FormKey form) = 0;
};
class PlayerCharacter {
public:
    explicit PlayerCharacter(WorldState& world) : world_(world) {}
    std::optional<FormKey> form_key() { return world_.player(); }
    std::optional<Position> position() {
        const auto id = world_.player();
        return id ? world_.reference_position(*id) : std::nullopt;
    }
    std::optional<float> actor_value(const char* name) {
        const auto id = world_.player();
        return id ? world_.actor_value(*id, name) : std::nullopt;
    }
private:
    WorldState& world_;
};
class ProcessLists {
public:
    explicit ProcessLists(const WorldState& world) : world_(world) {}
    std::vector<FormKey> actor_forms() const { return world_.loaded_actors(); }
private:
    const WorldState& world_;
};
class Projectile {
public:
    explicit Projectile(ProjectileHandle handle) : handle_(handle) {}
    UnsupportedHook runtime() const noexcept { return UnsupportedHook::ProjectileRuntime; }
    ProjectileHandle handle() const noexcept { return handle_; }
private:
    ProjectileHandle handle_;
};
struct IFormFactory {
    std::string name;
    FormType form_type;
    FormType GetFormType() const noexcept { return form_type; }
    const char* GetFormName() const noexcept { return name.c_str(); }
    UnsupportedHook Create() const noexcept { return UnsupportedHook::FormCreation; }
};
struct InventoryEntryData {
    FormId object;
    std::int32_t count_delta;
    std::vector<std::uint8_t> extra_types;
    bool worn;
};
struct InventoryChanges {
    FormId owner;
    std::vector<InventoryEntryData> entries;
    std::vector<std::pair<FormId, std::int32_t>> totals;
    std::int32_t GetItemCount(FormId object) const {
        for (const auto& total : totals) if (total.first.value == object.value) return total.second;
        return 0;
    }
    UnsupportedHook AddEntryData(const InventoryEntryData&) const noexcept { return UnsupportedHook::InventoryMutation; }
};
struct ItemList {
    std::vector<InventoryEntryData> items;
    UnsupportedHook GetSelectedItem() const noexcept { return UnsupportedHook::ItemSelection; }
    UnsupportedHook Update() const noexcept { return UnsupportedHook::ItemListUpdate; }
};
enum class ReferenceKind { Actor, Reference };
struct LookupReferenceByHandleResult { FormId form_id; ReferenceKind kind; };
class ReferenceResolver {
public:
    virtual ~ReferenceResolver() = default;
    virtual bool lookup_reference_by_handle(ObjectRefHandle handle,
                                            LookupReferenceByHandleResult& result) const = 0;
};
inline bool LookupReferenceByHandle(const ReferenceResolver& resolver, ObjectRefHandle handle,
                                    LookupReferenceByHandleResult& result) {
    return resolver.lookup_reference_by_handle(handle, result);
}
} // namespace RE
