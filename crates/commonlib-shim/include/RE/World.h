// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Reference.h"
#include <cstdint>
#include <optional>
#include <vector>

namespace RE {
struct FormKey { std::uint16_t file; std::uint32_t object; };
struct ProjectileHandle { std::uint32_t value; };
struct Position { float x; float y; float z; };
enum class UnsupportedHook { ProjectileRuntime, BinaryPatch };
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
} // namespace RE
