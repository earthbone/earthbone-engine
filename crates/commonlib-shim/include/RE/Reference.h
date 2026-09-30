// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
#pragma once
#include "RE/Math.h"
#include "RE/Form.h"
#include "RE/World.h"
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct ObjectRefHandle { std::uint32_t value; };
struct ActorHandle {
    std::uint32_t value;
    constexpr operator ObjectRefHandle() const noexcept { return {value}; }
};
struct ActorPtr { std::optional<ActorHandle> handle; };
struct ActorState {
    bool dead{};
    bool unconscious{};
    bool sneaking{};
    bool weapon_drawn{};
    bool blocking{};
};
enum class HookError { AddressPatch, CallDetour, VtableOverride, Trampoline };
enum class ActorApiError { InvalidActor, UnknownForm, UnsupportedOperation };
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
    virtual ActorPtr actor_ptr() const { return {actor_handle()}; }
    virtual ActorState state() const = 0;
    virtual std::optional<float> actor_value(ActorValue av) const = 0;
    virtual std::optional<ActorApiError> set_actor_value(ActorValue av, float value) = 0;
    virtual FormId race() const = 0;
    virtual bool is_dead() const = 0;
    virtual bool is_player() const = 0;
    virtual bool add_spell(FormId spell) = 0;
    virtual bool add_perk(FormId perk) = 0;
    virtual bool add_item(FormId item, std::uint32_t count) = 0;
};

class ActorEquipManager {
public:
    virtual ~ActorEquipManager() = default;
    virtual std::optional<ActorApiError> equip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<ActorApiError> unequip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<HookError> reject_hook(HookError hook) const = 0;
};
} // namespace RE
