// Generated from crates/commonlib-shim/src/extra_data.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Math.h"
#include "RE/Reference.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
struct ActivateParentID {
    FormId refID;
    float activateDelay;
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

class ReferenceExtraData {
public:
    virtual ~ReferenceExtraData() = default;
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
