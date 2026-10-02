// SPDX-License-Identifier: MIT
#include <RE/Conditions.h>
#include <cassert>
#include <iomanip>
#include <iostream>

int main() {
    RE::CONDITION_ITEM_DATA item{};
    item.function = 999;
    item.object = RE::CONDITIONITEMOBJECT::kSelf;
    RE::TESCondition conditions;
    conditions.items.push_back({item});
    RE::ConditionRuntimeBridge bridge(std::cin, std::cout);
    RE::ConditionContext context{};
    context.owner.value = 0x14;
    context.subject.value = 0x14;
    bool result = false;
    std::cout << std::setprecision(9);
    const auto item_error = conditions.items.front().IsTrue(bridge, context, result);
    const auto error = conditions.IsTrue(bridge, context, result);
    const auto item_copy_error = conditions.items.front().Copy();
    const auto list_copy_error = conditions.Copy();
    assert(item_error.code == RE::ConditionErrorCode::UnsupportedFunction);
    assert(item_error.detail == 999);
    assert(error.code == RE::ConditionErrorCode::UnsupportedFunction);
    assert(error.detail == 999);
    assert(!result);
    assert(item_copy_error.code == RE::ConditionErrorCode::UnsupportedHook);
    assert(list_copy_error.code == RE::ConditionErrorCode::UnsupportedHook);
}
