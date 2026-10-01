// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::{conditions::*, form::FormId, refr::ObjectRefHandle};

struct Runtime { calls: Vec<u16> }
impl ConditionRuntime for Runtime {
    fn evaluate_item(&mut self, item: &CONDITION_ITEM_DATA, _: ConditionContext) -> Result<bool, ConditionError> {
        self.calls.push(item.function);
        if item.function == 9 { Err(ConditionError::UnsupportedFunction(9)) } else { Ok(item.function == 1) }
    }
}
fn item(function: u16, flags: u8) -> TESConditionItem {
    TESConditionItem { data: CONDITION_ITEM_DATA { flags, comparison: ComparisonValue::Number(0.0), function, parameters: [0; 2], run_on: 0, reference: FormId(0), parameter_3: -1, object: CONDITIONITEMOBJECT::SelfRef } }
}
fn context() -> ConditionContext { ConditionContext { owner: FormId(1), subject: ObjectRefHandle(2), target: None, quest: None } }

#[test]
fn empty_condition_is_true_without_calling_runtime() {
    let mut runtime = Runtime { calls: vec![] };
    assert!(TESCondition::default().is_true(&mut runtime, context()).unwrap());
    assert!(runtime.calls.is_empty());
}

#[test]
fn false_and_group_short_circuits_later_or_group_and_preserves_error_if_reached() {
    let mut runtime = Runtime { calls: vec![] };
    let condition = TESCondition { items: vec![item(2, 0), item(9, 0)] };
    assert!(!condition.is_true(&mut runtime, context()).unwrap());
    assert_eq!(runtime.calls, [2]);
}
