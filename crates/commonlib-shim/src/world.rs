// SPDX-License-Identifier: MIT

//! World singleton and extra-data contracts backed by the engine's live form
//! table. The contracts describe values, not Skyrim memory layouts or hooks.

use crate::form::FormId;

/// Failure for a CommonLib operation that depends on a Skyrim executable hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedHook {
    ActivateReferenceChildren,
}

/// Calendar values exposed by the running game.
pub trait Calendar {
    fn year(&self) -> u32;
    fn month(&self) -> u32;
    fn day(&self) -> u32;
    fn hour(&self) -> f64;
    fn days_passed(&self) -> f64;
    fn timescale(&self) -> f64;
}

/// Actor state exposed through the engine's actor component.
pub trait Character {
    fn form_id(&self) -> FormId;
    fn actor_value(&self, name: &str) -> Option<f32>;
    fn is_dead(&self) -> bool;
    fn race(&self) -> Option<FormId>;
}

/// Sparse `DOBJ` lookup by the four byte type key.
pub trait DefaultObjects {
    fn get(&self, key: [u8; 4]) -> Result<Option<FormId>, String>;
}

/// The runtime reference-alias data represented by ExtraAliasInstanceArray.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasInstance {
    pub quest: FormId,
    pub alias_id: u32,
    pub reference: Option<FormId>,
    pub packages: Vec<FormId>,
}

pub trait AliasInstances {
    fn instances(&self, reference: FormId) -> Vec<AliasInstance>;
}

/// Child activation data is not currently stored by the engine runtime.
/// Callers receive an explicit unsupported result rather than a false empty list.
pub trait ActivateRefChildren {
    fn children(&self, reference: FormId) -> Result<Vec<FormId>, UnsupportedHook>;
    fn timer(&self, reference: FormId) -> Result<f32, UnsupportedHook>;
    fn activate_children(&self, reference: FormId) -> Result<(), UnsupportedHook>;
}
