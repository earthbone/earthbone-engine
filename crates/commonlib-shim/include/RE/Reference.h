// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
#pragma once
#include "RE/Math.h"
#include "RE/Form.h"
#include "RE/World.h"
#include "RE/WorldTypes.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

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
enum class ActorHookError { AddressPatch, CallDetour, VtableOverride, Trampoline };
enum class ActorApiError { InvalidActor, UnknownForm, UnsupportedOperation };

struct ExtraLeveledItemBase { FormId leveled_item; };
struct ExtraLightData {
    float fov;
    float fade;
    float end_distance_cap;
    float shadow_depth_bias;
    std::uint8_t unknown;
};
struct LinkedRefChild { FormId keyword; ObjectRefHandle reference; };
struct ExtraMapMarker { std::string name; std::uint8_t flags; std::uint16_t marker_type; };
struct MissingLinkedRefId { FormId keyword; FormId linked_reference; };

class ReferenceExtraData {
public:
    virtual ~ReferenceExtraData() = default;
    virtual std::optional<ExtraLeveledItemBase> leveled_item_base() const = 0;
    virtual std::optional<ExtraLightData> light_data() const = 0;
    virtual const std::vector<LinkedRefChild>& linked_ref_children() const = 0;
    virtual std::optional<ExtraMapMarker> map_marker() const = 0;
    virtual const std::vector<MissingLinkedRefId>& missing_linked_ref_ids() const = 0;
};
struct ReferenceUnsupportedHookInfo { const char* name; const char* reason; };
inline constexpr ReferenceUnsupportedHookInfo EXTRA_DATA_HOOKS{
    "CommonLib extra-data vtable hooks",
    "Skyrim executable vtables have no target in the Rust engine"};
enum class WEAPON_STATE : std::uint32_t {
    kSheathed = 0, kWantToDraw = 1, kDrawing = 2,
    kDrawn = 3, kWantToSheathe = 4, kSheathing = 5
};
struct Unsupported {
    const char* operation;
    std::optional<UnsupportedHook> hook;
    Unsupported(const char* operation) : operation(operation), hook(std::nullopt) {}
    Unsupported(UnsupportedHook hook) : operation(nullptr), hook(hook) {}
};
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
    virtual ActorPtr actor_ptr() const = 0;
    virtual ActorState state() const = 0;
    virtual std::optional<float> actor_value(ActorValue av) const = 0;
    virtual std::optional<ActorApiError> set_actor_value(ActorValue av, float value) = 0;
    virtual FormId race() const = 0;
    virtual bool is_dead() const = 0;
    virtual bool is_player() const = 0;
    virtual WEAPON_STATE weapon_state() const = 0;
    virtual bool add_spell(FormId spell) = 0;
    virtual bool add_perk(FormId perk) = 0;
    virtual bool add_item(FormId item, std::uint32_t count) = 0;
};

class ActorEquipManager {
public:
    virtual ~ActorEquipManager() = default;
    virtual std::optional<ActorApiError> equip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<ActorApiError> unequip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<ActorHookError> reject_hook(ActorHookError hook) const = 0;
};
} // namespace RE
