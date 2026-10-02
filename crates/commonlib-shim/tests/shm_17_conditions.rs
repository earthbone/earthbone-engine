// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{fs, path::PathBuf, process::Command};

use soe_commonlib_shim::conditions::{
    ComparisonValue, ConditionContext, ConditionError, ConditionHook, ConditionRuntime,
    TESCondition, TESConditionItem, CONDITIONITEMOBJECT, CONDITION_ITEM_DATA,
};
use soe_commonlib_shim::{form::FormId, refr::ObjectRefHandle};

fn condition(function: u16, flags: u8) -> TESConditionItem {
    TESConditionItem {
        data: CONDITION_ITEM_DATA {
            flags,
            comparison: ComparisonValue::Number(1.0),
            function,
            parameters: [0, 0],
            run_on: 0,
            reference: FormId(0),
            parameter_3: -1,
            object: CONDITIONITEMOBJECT::SelfRef,
        },
    }
}

struct FixtureRuntime {
    calls: Vec<u16>,
}

impl ConditionRuntime for FixtureRuntime {
    fn evaluate_item(
        &mut self,
        item: &CONDITION_ITEM_DATA,
        _: ConditionContext,
    ) -> Result<bool, ConditionError> {
        self.calls.push(item.function);
        Ok(item.function == 2)
    }
}

#[test]
fn shm_17_condition_list_uses_runtime_state_and_or_groups() {
    let condition = TESCondition {
        items: vec![condition(1, 1), condition(2, 0)],
    };
    let mut runtime = FixtureRuntime { calls: Vec::new() };
    let context = ConditionContext {
        owner: FormId(0x800),
        subject: ObjectRefHandle(0x900),
        target: None,
        quest: None,
    };
    assert!(condition.is_true(&mut runtime, context).unwrap());
    assert_eq!(runtime.calls, [1, 2]);
}

#[test]
fn shm_17_condition_copy_hook_returns_explicit_unsupported_error() {
    assert_eq!(
        condition(1, 0).copy().unwrap_err(),
        ConditionError::UnsupportedHook(ConditionHook::Copy)
    );
    assert_eq!(
        TESCondition::default().copy().unwrap_err(),
        ConditionError::UnsupportedHook(ConditionHook::Copy)
    );
}

#[test]
fn shm_17_cpp_condition_contract_matches_the_live_runtime_boundary() {
    let include = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include");
    let scratch = std::env::temp_dir().join(format!("soe-shm-17-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("conditions.cpp");
    fs::write(
        &source,
        "
#include <RE/Skyrim.h>
#include <concepts>
#include <type_traits>
static_assert(static_cast<unsigned>(RE::CONDITIONITEMOBJECT::kSelf) == 0);
static_assert(static_cast<unsigned>(RE::CONDITIONITEMOBJECT::kCommandTarget) == 8);
static_assert(std::same_as<decltype(RE::CONDITION_ITEM_DATA::function), std::uint16_t>);
static_assert(std::same_as<decltype(RE::CONDITION_ITEM_DATA::parameters), std::uint32_t[2]>);
static_assert(std::same_as<decltype(&RE::ConditionRuntime::evaluate_item),
    RE::ConditionError (RE::ConditionRuntime::*)(const RE::CONDITION_ITEM_DATA&,
    const RE::ConditionContext&, bool&)>);
static_assert(std::same_as<decltype(&RE::TESConditionItem::is_true),
    RE::ConditionError (RE::TESConditionItem::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
static_assert(std::same_as<decltype(&RE::TESConditionItem::IsTrue),
    RE::ConditionError (RE::TESConditionItem::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
static_assert(std::same_as<decltype(&RE::TESConditionItem::operator()),
    RE::ConditionError (RE::TESConditionItem::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
static_assert(std::same_as<decltype(&RE::TESCondition::is_true),
    RE::ConditionError (RE::TESCondition::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
static_assert(std::same_as<decltype(&RE::TESCondition::IsTrue),
    RE::ConditionError (RE::TESCondition::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
static_assert(std::same_as<decltype(&RE::TESCondition::operator()),
    RE::ConditionError (RE::TESCondition::*)(RE::ConditionRuntime&,
    const RE::ConditionContext&, bool&) const>);
struct LiveState final : RE::ConditionRuntime {
    RE::ConditionError evaluate_item(const RE::CONDITION_ITEM_DATA& item,
        const RE::ConditionContext&, bool& result) override {
        result = item.function == 14;
        return {RE::ConditionErrorCode::None, 0};
    }
};
void contract(RE::TESCondition& conditions, RE::ConditionContext& context) {
    LiveState state;
    bool result = false;
    (void)conditions.IsTrue(state, context, result);
    (void)conditions(state, context, result);
    (void)conditions.items.front().IsTrue(state, context, result);
    (void)conditions.items.front()(state, context, result);
    const auto unsupported = conditions.Copy();
    (void)unsupported;
}
",
    )
    .unwrap();
    let output = Command::new("c++")
        .arg("-std=c++20")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(include)
        .arg(source)
        .output()
        .expect("a C++20 compiler is required by the SHM-17 done check");
    assert!(
        output.status.success(),
        "C++ condition contract failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(scratch).unwrap();
}
