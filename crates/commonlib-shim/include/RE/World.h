// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <array>
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct ActorValue {
    std::uint32_t value;
    static const ActorValue HEALTH;
    static const ActorValue MAGICKA;
    static const ActorValue STAMINA;
};
inline const ActorValue ActorValue::HEALTH{24};
inline const ActorValue ActorValue::MAGICKA{25};
inline const ActorValue ActorValue::STAMINA{26};
enum class UnsupportedHook { ArrowProjectile, LoadFormBuffer };

class ActorValueWorldState {
public:
    virtual ~ActorValueWorldState() = default;
    virtual std::optional<float> actor_value(std::uint32_t actor, ActorValue value) const = 0;
    virtual bool set_actor_value(std::uint32_t actor, ActorValue value, float amount) = 0;
    virtual std::optional<std::string> actor_value_name(ActorValue value) const = 0;
    virtual std::optional<ActorValue> actor_value_by_name(const std::string& name) const = 0;
    virtual std::optional<FormId> default_object(std::array<char, 4> key) const = 0;
};

class ActorValueList {
public:
    explicit ActorValueList(ActorValueWorldState& state) : state_(state) {}
    std::optional<float> get_actor_value_info(std::uint32_t actor, ActorValue value) const {
        return state_.actor_value(actor, value);
    }
    bool set_actor_value(std::uint32_t actor, ActorValue value, float amount) {
        return state_.set_actor_value(actor, value, amount);
    }
    std::optional<std::string> get_actor_value_info(ActorValue value) const {
        return state_.actor_value_name(value);
    }
    std::optional<std::string> get_actor_value_name(ActorValue value) const {
        return state_.actor_value_name(value);
    }
    std::optional<ActorValue> lookup_actor_value_by_name(const std::string& name) const {
        return state_.actor_value_by_name(name);
    }
private:
    ActorValueWorldState& state_;
};

class BGSDefaultObjectManager {
public:
    explicit BGSDefaultObjectManager(const ActorValueWorldState& state) : state_(state) {}
    std::optional<FormId> get(std::array<char, 4> key) const { return state_.default_object(key); }
private:
    const ActorValueWorldState& state_;
};

struct ArrowProjectile {
    static constexpr UnsupportedHook save_game() noexcept { return UnsupportedHook::ArrowProjectile; }
    static constexpr UnsupportedHook load_game() noexcept { return UnsupportedHook::ArrowProjectile; }
    static constexpr UnsupportedHook process_impacts() noexcept { return UnsupportedHook::ArrowProjectile; }
};
struct BGSLoadFormBuffer {
    static constexpr UnsupportedHook get_version() noexcept { return UnsupportedHook::LoadFormBuffer; }
    static constexpr UnsupportedHook load_form() noexcept { return UnsupportedHook::LoadFormBuffer; }
};
enum class ACTOR_CRITICAL_STAGE : std::uint32_t { kNone = 0, kGooStart = 1, kGooEnd = 2, kDisintegrateStart = 3, kDisintegrateEnd = 4 };
enum class ACTOR_VALUE_MODIFIER : std::uint32_t { kPermanent = 0, kTemporary = 1, kDamage = 2 };
struct ActivateParentID { FormId refID; float activateDelay; };
struct AIProcess { std::optional<FormId> runningPackage; std::string procedure; };
struct ActiveEffect {
    std::uint64_t id;
    FormId spell;
    FormId effect;
    FormId source;
    std::optional<FormId> caster;
    FormId target;
    float magnitude;
    std::optional<double> duration;
    double elapsed;
};
struct UnsupportedState { const char* message; };
class WorldState {
public:
    virtual ~WorldState() = default;
    virtual ACTOR_CRITICAL_STAGE critical_stage(FormId actor) = 0;
    virtual bool set_critical_stage(FormId actor, ACTOR_CRITICAL_STAGE stage) = 0;
    virtual float actor_value_modifier(FormId actor, std::uint32_t actorValue, ACTOR_VALUE_MODIFIER modifier) = 0;
    virtual AIProcess ai_process(FormId actor) = 0;
    virtual std::vector<ActivateParentID> activate_parents(FormId reference) = 0;
    virtual std::optional<ActiveEffect> active_effect(std::uint64_t id) const = 0;
    virtual UnsupportedState unsupported_hook(const std::string& name) const = 0;
};
} // namespace RE
