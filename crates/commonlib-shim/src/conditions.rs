// SPDX-License-Identifier: MIT

//! Condition values and the runtime boundary used by the CommonLib-shaped API.

use crate::{form::FormId, refr::ObjectRefHandle};

/// The condition subject selector from CommonLib's `CONDITIONITEMOBJECT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum CONDITIONITEMOBJECT {
    /// The condition's action reference.
    SelfRef = 0,
    /// The condition's target reference.
    Target = 1,
    /// An authored reference.
    Ref = 2,
    /// The combat target.
    CombatTarget = 3,
    /// A linked reference.
    LinkedRef = 4,
    /// A quest alias.
    QuestAlias = 5,
    /// Package data.
    PackData = 6,
    /// Event data.
    EventData = 7,
    /// The command target.
    CommandTarget = 8,
}

/// Numeric or global-form comparison value used by one condition item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ComparisonValue {
    /// Compare against a literal number.
    Number(f32),
    /// Compare against a global form.
    Global(FormId),
}

/// The caller's live reference context for condition evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionContext {
    /// The form that owns the condition data.
    pub owner: FormId,
    /// The action reference.
    pub subject: ObjectRefHandle,
    /// The optional target reference.
    pub target: Option<ObjectRefHandle>,
    /// The optional quest used for quest aliases.
    pub quest: Option<FormId>,
}

/// One parsed condition item in engine-independent form.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub struct CONDITION_ITEM_DATA {
    /// Operator and condition flags in the CTDA byte.
    pub flags: u8,
    /// Numeric or global comparison value.
    pub comparison: ComparisonValue,
    /// xEdit condition function index.
    pub function: u16,
    /// Function-specific raw parameter unions.
    pub parameters: [u32; 2],
    /// CommonLib run-on selector.
    pub run_on: u32,
    /// Explicit run-on reference when selected.
    pub reference: FormId,
    /// Third parameter, used by alias-based conditions.
    pub parameter_3: i32,
    /// CommonLib condition object selector.
    pub object: CONDITIONITEMOBJECT,
}

/// Unsupported condition operations are reported instead of silently succeeding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionError {
    /// The requested condition function has no game implementation.
    UnsupportedFunction(u16),
    /// The requested run-on target is unavailable.
    UnsupportedTarget(u32),
    /// The requested shim hook has no engine equivalent.
    UnsupportedHook(ConditionHook),
    /// A referenced form or variable could not be resolved.
    UnavailableState,
    /// The condition payload has invalid flags, values, or operators.
    InvalidCondition,
}

/// Hooks exposed by CommonLib whose native allocation behavior is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionHook {
    /// Copying a native linked condition list is not supported.
    Copy,
}

/// The engine-owned state used by condition calls.
pub trait ConditionRuntime {
    /// Evaluate one item against current engine state.
    fn evaluate_item(
        &mut self,
        item: &CONDITION_ITEM_DATA,
        context: ConditionContext,
    ) -> Result<bool, ConditionError>;
}

/// A single linked-list item in the CommonLib condition model.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(non_camel_case_types)]
pub struct TESConditionItem {
    /// Condition payload.
    pub data: CONDITION_ITEM_DATA,
}

impl TESConditionItem {
    /// Evaluate this item against the caller's live state.
    pub fn is_true<R: ConditionRuntime>(
        &self,
        runtime: &mut R,
        context: ConditionContext,
    ) -> Result<bool, ConditionError> {
        runtime.evaluate_item(&self.data, context)
    }

    /// Report that native condition-list copying is unsupported.
    pub fn copy(&self) -> Result<(), ConditionError> {
        Err(ConditionError::UnsupportedHook(ConditionHook::Copy))
    }
}

/// An ordered CommonLib condition list with the CTDA AND/OR grouping rule.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(non_camel_case_types)]
pub struct TESCondition {
    /// Items in source order.
    pub items: Vec<TESConditionItem>,
}

impl TESCondition {
    /// Evaluate ordered conditions, joining an OR-marked item to its successor.
    pub fn is_true<R: ConditionRuntime>(
        &self,
        runtime: &mut R,
        context: ConditionContext,
    ) -> Result<bool, ConditionError> {
        let mut any = false;
        for (index, item) in self.items.iter().enumerate() {
            if !any {
                any = item.is_true(runtime, context)?;
            }
            if item.data.flags & 1 == 0 || index + 1 == self.items.len() {
                if !any {
                    return Ok(false);
                }
                any = false;
            }
        }
        Ok(true)
    }

    /// Report that native condition-list copying is unsupported.
    pub fn copy(&self) -> Result<(), ConditionError> {
        Err(ConditionError::UnsupportedHook(ConditionHook::Copy))
    }
}
