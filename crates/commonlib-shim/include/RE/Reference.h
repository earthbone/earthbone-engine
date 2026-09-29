// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
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
enum class WEAPON_STATE : std::uint32_t {
    kSheathed = 0, kWantToDraw = 1, kDrawing = 2,
    kDrawn = 3, kWantToSheathe = 4, kSheathing = 5
};
struct Unsupported { const char* operation; };
struct TESObjectREFRPtr {
    std::optional<ObjectRefHandle> handle;
    bool is_null() const noexcept { return !handle.has_value(); }
};

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
    virtual Unsupported set_base(FormId base) = 0;
};
using TESObjectREFR = ObjectRef;

class Actor : public ObjectRef {
public:
    virtual ActorHandle actor_handle() const = 0;
    virtual float actor_value(ActorValue av) const = 0;
    virtual Unsupported set_actor_value(ActorValue av, float value) = 0;
    virtual FormId race() const = 0;
    virtual bool is_dead() const = 0;
    virtual bool is_player() const = 0;
    virtual WEAPON_STATE weapon_state() const = 0;
    virtual bool add_spell(FormId spell) = 0;
    virtual bool add_perk(FormId perk) = 0;
    virtual bool add_item(FormId item, std::uint32_t count) = 0;
};
} // namespace RE
