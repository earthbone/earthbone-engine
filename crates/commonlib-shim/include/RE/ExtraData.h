// Generated from crates/commonlib-shim/src/extra.rs and extra_data.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Math.h"
#include "RE/Reference.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
enum class ExtraDataType : std::uint8_t { LeveledCreature = 0x2D, Hotkey = 0x49, Enchantment = 0x9B };
struct ExtraEnchantment { FormId enchantment; std::uint16_t charge; std::optional<bool> remove_on_unequip; friend bool operator==(const ExtraEnchantment&, const ExtraEnchantment&) = default; };
struct ExtraHotkey { std::optional<std::uint8_t> slot; friend bool operator==(const ExtraHotkey&, const ExtraHotkey&) = default; };
struct ExtraLeveledCreature { FormId original_base; FormId template_base; friend bool operator==(const ExtraLeveledCreature&, const ExtraLeveledCreature&) = default; };

class ExtraDataSource {
public:
    virtual ~ExtraDataSource() = default;
    virtual bool has_type(ExtraDataType kind) const = 0;
    virtual std::optional<ExtraEnchantment> enchantment() const = 0;
    virtual std::optional<ExtraHotkey> hotkey() const = 0;
    virtual std::optional<ExtraLeveledCreature> leveled_creature() const = 0;
};
class ExtraDataList {
public:
    explicit ExtraDataList(const ExtraDataSource& source) : source_(source) {}
    bool has_type(ExtraDataType kind) const { return source_.has_type(kind); }
    std::optional<ExtraEnchantment> enchantment() const { return source_.enchantment(); }
    std::optional<ExtraHotkey> hotkey() const { return source_.hotkey(); }
    std::optional<ExtraLeveledCreature> leveled_creature() const { return source_.leveled_creature(); }
    static constexpr const char* ADD_UNSUPPORTED = "ExtraDataList::Add mutates engine-owned list state";
    static constexpr const char* REMOVE_UNSUPPORTED = "ExtraDataList::Remove mutates engine-owned list state";
    static constexpr const char* REMOVE_BY_TYPE_UNSUPPORTED = "ExtraDataList::RemoveByType mutates engine-owned list state";
    static constexpr const char* SET_ENCHANTMENT_UNSUPPORTED = "ExtraDataList::SetEnchantment mutates engine-owned list state";
private:
    const ExtraDataSource& source_;
};

struct DoorTeleportData {
    ObjectRefHandle linkedDoor;
    NiPoint3 position;
    NiPoint3 rotation;
    std::int8_t flags;
};

enum class ReferenceExtraKind { MissingRefIDs, NorthRotation, OutfitItem, Poison, Teleport };
struct ReferenceExtraValue {
    ReferenceExtraKind kind;
    std::vector<ActivateParentID> missingRefIDs;
    std::optional<float> northRotation;
    std::optional<FormId> outfitItem;
    std::optional<FormId> poison;
    std::int32_t poisonCount{};
    std::optional<DoorTeleportData> teleport;
};

class ReferenceExtraQuery {
public:
    virtual ~ReferenceExtraQuery() = default;
    virtual bool supports_extra(ReferenceExtraKind kind) const = 0;
    virtual std::optional<ReferenceExtraValue> extra(ObjectRefHandle reference, ReferenceExtraKind kind) const = 0;
};

class HookRegistry {
public:
    static bool reject(const std::string& name, std::string& error) {
        error = "unsupported binary hook: " + name;
        return false;
    }
};
} // namespace RE
