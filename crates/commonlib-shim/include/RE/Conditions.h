// Generated from crates/commonlib-shim/src/conditions.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Reference.h"
#include <cstddef>
#include <cstdint>
#include <vector>

namespace RE {
enum class CONDITIONITEMOBJECT : std::uint8_t {
    kSelf = 0, kTarget = 1, kRef = 2, kCombatTarget = 3, kLinkedRef = 4,
    kQuestAlias = 5, kPackData = 6, kEventData = 7, kCommandTarget = 8
};
enum class ConditionErrorCode : std::uint8_t {
    None, UnsupportedFunction, UnsupportedTarget, UnsupportedHook, UnavailableState, InvalidCondition
};
enum class ConditionHook : std::uint8_t { Copy };
struct ConditionError { ConditionErrorCode code; std::uint32_t detail; };
struct ComparisonValue {
    bool is_global;
    float number;
    FormId global;
};
struct ConditionContext {
    FormId owner;
    ObjectRefHandle subject;
    ObjectRefHandle target;
    bool has_target;
    FormId quest;
    bool has_quest;
};
struct CONDITION_ITEM_DATA {
    std::uint8_t flags;
    ComparisonValue comparison;
    std::uint16_t function;
    std::uint32_t parameters[2];
    std::uint32_t run_on;
    FormId reference;
    std::int32_t parameter_3;
    CONDITIONITEMOBJECT object;
};
class ConditionRuntime {
public:
    virtual ~ConditionRuntime() = default;
    virtual ConditionError evaluate_item(const CONDITION_ITEM_DATA& item,
                                         const ConditionContext& context,
                                         bool& result) = 0;
};
struct TESConditionItem {
    CONDITION_ITEM_DATA data;
    ConditionError IsTrue(ConditionRuntime& runtime, const ConditionContext& context,
                          bool& result) const {
        return runtime.evaluate_item(data, context, result);
    }
    ConditionError operator()(ConditionRuntime& runtime, const ConditionContext& context,
                              bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError is_true(ConditionRuntime& runtime, const ConditionContext& context,
                           bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError Copy() const { return copy(); }
    ConditionError copy() const {
        return {ConditionErrorCode::UnsupportedHook,
                static_cast<std::uint32_t>(ConditionHook::Copy)};
    }
};
struct TESCondition {
    std::vector<TESConditionItem> items;
    ConditionError IsTrue(ConditionRuntime& runtime, const ConditionContext& context,
                          bool& result) const {
        return is_true(runtime, context, result);
    }
    ConditionError operator()(ConditionRuntime& runtime, const ConditionContext& context,
                              bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError is_true(ConditionRuntime& runtime, const ConditionContext& context,
                           bool& result) const {
        bool any = false;
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = items[index];
            if (!any) {
                bool value = false;
                auto error = item.is_true(runtime, context, value);
                if (error.code != ConditionErrorCode::None) return error;
                any = value;
            }
            if ((item.data.flags & 1U) == 0 || index + 1 == items.size()) {
                if (!any) { result = false; return {}; }
                any = false;
            }
        }
        result = true;
        return {};
    }
    ConditionError Copy() const { return copy(); }
    ConditionError copy() const {
        return {ConditionErrorCode::UnsupportedHook,
                static_cast<std::uint32_t>(ConditionHook::Copy)};
    }
};
} // namespace RE
