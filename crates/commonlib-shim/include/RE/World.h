// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Reference.h"
#include "RE/WorldTypes.h"
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace RE {
struct IFormFactory {
    std::string name;
    FormType form_type;
    FormType GetFormType() const noexcept { return form_type; }
    const char* GetFormName() const noexcept { return name.c_str(); }
    UnsupportedHook Create() const noexcept { return UnsupportedHook::FormCreation; }
};
struct InventoryEntryData {
    FormId object;
    std::int32_t count_delta;
    std::vector<std::uint8_t> extra_types;
    bool worn;
};
struct InventoryChanges {
    FormId owner;
    std::vector<InventoryEntryData> entries;
    std::vector<std::pair<FormId, std::int32_t>> totals;
    std::int32_t GetItemCount(FormId object) const {
        for (const auto& total : totals) if (total.first.value == object.value) return total.second;
        return 0;
    }
    UnsupportedHook AddEntryData(const InventoryEntryData&) const noexcept { return UnsupportedHook::InventoryMutation; }
};
struct ItemList {
    std::vector<InventoryEntryData> items;
    UnsupportedHook GetSelectedItem() const noexcept { return UnsupportedHook::ItemSelection; }
    UnsupportedHook Update() const noexcept { return UnsupportedHook::ItemListUpdate; }
};
enum class ReferenceKind { Actor, Reference };
struct LookupReferenceByHandleResult { FormId form_id; ReferenceKind kind; };
class ReferenceResolver {
public:
    virtual ~ReferenceResolver() = default;
    virtual bool lookup_reference_by_handle(ObjectRefHandle handle,
                                            LookupReferenceByHandleResult& result) const = 0;
};
inline bool LookupReferenceByHandle(const ReferenceResolver& resolver, ObjectRefHandle handle,
                                    LookupReferenceByHandleResult& result) {
    return resolver.lookup_reference_by_handle(handle, result);
}
} // namespace RE
