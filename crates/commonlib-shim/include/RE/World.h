// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
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
