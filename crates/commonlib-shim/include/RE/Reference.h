// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
#pragma once
#include "RE/Math.h"
#include "RE/Form.h"
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
struct ActorValue {
    std::uint32_t value;
    static const ActorValue HEALTH;
    static const ActorValue MAGICKA;
    static const ActorValue STAMINA;
};
inline const ActorValue ActorValue::HEALTH{24};
inline const ActorValue ActorValue::MAGICKA{25};
inline const ActorValue ActorValue::STAMINA{26};

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
struct UnsupportedHook { const char* name; const char* reason; };
inline constexpr UnsupportedHook EXTRA_DATA_HOOKS{
    "CommonLib extra-data vtable hooks",
    "Skyrim executable vtables have no target in the Rust engine"};

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
