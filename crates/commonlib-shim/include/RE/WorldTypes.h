// Generated from crates/commonlib-shim/src/world_types.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Math.h"
#include <optional>
#include <string>

namespace RE {
enum class UnsupportedHook {
    FormCreation, InventoryMutation, ItemSelection, ItemListUpdate,
    BinaryAddress, VtablePatch, UnsupportedEquipOption,
    ProjectileRuntime, BinaryPatch, ArrowProjectile, LoadFormBuffer, ActivateReferenceChildren
};
struct SettingValue {
    enum class Kind { Bool, Int, Float, String };
    Kind kind;
    bool boolean_value{};
    std::int32_t int_value{};
    float float_value{};
    std::string string_value;
};
struct WornState { bool worn; bool worn_left; };
struct HazardState { FormId reference; FormId base; NiPoint3 position; bool enabled; };
struct HighProcessState {
    FormId actor;
    bool in_combat;
    std::optional<FormId> combat_target;
    bool staggered;
};
struct HitData {
    FormId attacker;
    FormId target;
    FormId weapon;
    float weapon_damage;
    float armor_reduction;
    float health_damage;
    bool blocked;
    bool killed;
};
struct UnsupportedHookInfo { std::string type_name; std::string hook_name; std::string reason; };

class GameSettingCollection {
public:
    virtual ~GameSettingCollection() = default;
    virtual std::optional<SettingValue> get_setting(const std::string& name) const = 0;
};
class WorldTypes {
public:
    virtual ~WorldTypes() = default;
    virtual std::optional<WornState> extra_worn(FormId actor, FormId item) const = 0;
    virtual std::optional<HazardState> hazard(FormId reference) const = 0;
    virtual std::optional<HighProcessState> high_process(FormId actor) const = 0;
    virtual std::optional<HitData> resolve_hit(FormId attacker, FormId target, FormId weapon) = 0;
    virtual UnsupportedHookInfo unsupported_hook(const std::string& type_name, const std::string& hook_name) const = 0;
};
} // namespace RE
